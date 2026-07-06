//! Purpose:
//! Worker-mode HTTP loop: boot the PHP application once, then accept → set
//! request → invoke the compiled worker handler trampoline → take response →
//! send → GC cyclic collection (Phase 4) → unlink multipart temp files
//! (Phase 4). Replaces per-request re-execution of top-level PHP with a single
//! boot + repeated handler invocations (FrankenPHP-style).
//!
//! Called from:
//! - `crate::handler::elephc_web_worker_register` tail-calls
//!   `enter_worker_loop()` once the PHP boot has registered its handler.
//! - `crate::server::spawn_worker` calls `set_worker_config` in the forked child
//!   before the boot runs, so the loop has its listen/max-body/etc. available.
//!
//! Key details:
//! - Handler-thread affinity under `--handler-offload`: when enabled, the whole
//!   per-request PHP body (`run_worker_handler` + `cleanup_tmp_files` +
//!   `maybe_collect_cycles` + the `take_*` drains) runs on ONE dedicated
//!   `php-handler` thread (see `crate::offload`), fed a bounded mpsc job queue by
//!   the I/O thread; the I/O thread references no `request_state::` mutator and no
//!   `__rt_*` extern. Boot still runs on the main thread (it must —
//!   `enter_worker_loop` is reached from inside boot); the handler thread is
//!   spawned after boot completes, so `std::thread::spawn`'s happens-before makes
//!   the boot's heap/globals/statics visible to it. When off, the loop is
//!   single-threaded exactly as before. Handlers never overlap either way.
//! - The boot function (`WORKER_BOOT`) runs once, before the tokio runtime/loop
//!   starts. It executes the top-level PHP which ends by calling
//!   `elephc_web_worker_register` → `enter_worker_loop` (never returns).
//! - `set_worker_config` must be called in the child after fork and before boot,
//!   because `enter_worker_loop` is reached from within the boot's call stack.
//! - SO_REUSEPORT listener and gzip helper are reused from `crate::worker`.
//! - Phase 4 calls `__rt_gc_collect_cycles` after each handler invocation, gated
//!   by `--worker-gc-interval` (default 1 = every request, 0 = never, N = every
//!   N requests). Superglobal reset is performed by the compiled trampoline
//!   (`__rt_web_worker_request_reset`, Phase 2); Rust only resets response state
//!   (body/status/headers) as in the classic `--web` mode.
//! - Phase 4 unlinks multipart temp files registered via
//!   `elephc_web_register_tmp_file` after each request, since worker processes
//!   live long and the per-request PHP prelude keeps creating them.
//! - Crash-loop startup-vs-runtime distinction: the existing timing-based
//!   FAST_DEATH guard is adequate for worker mode. A runtime crash (worker has
//!   served requests, lived past FAST_DEATH) resets the fast-deaths counter and
//!   respawns immediately; a startup crash (died within FAST_DEATH of spawn)
//!   counts toward MAX_FAST_DEATHS and triggers exponential backoff-style giveup
//!   after too many. A pipe-based boot signal is documented as a future
//!   refinement; the timing heuristic is sufficient because workers that reach
//!   `elephc_web_worker_register` and serve at least one request reliably
//!   outlive the FAST_DEATH window.

use std::cell::Cell;
use std::convert::Infallible;
use std::ffi::{c_char, CString};
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioTimer;
use tokio::net::TcpListener;

use crate::handler;
use crate::offload::{self, RequestJob, ResponseParts};
use crate::request_state;
use crate::worker::{
    drive_connection, reuseport_listener, should_close_connection, ConnSource, NextConn,
    WorkerConfig,
};

// Runtime cycle collector provided by the compiled program's runtime. Called
// after each handler invocation (gated by `--worker-gc-interval`) to reclaim
// cyclic garbage that plain refcounting cannot free, while keeping statics and
// globals alive across requests. The compiler always emits this helper, so the
// symbol resolves at link time when the program is linked against elephc-web.
// In `cfg(test)` the program's runtime object is absent, so a no-op local stub
// (`rt_gc_collect_cycles_test_stub`) is used instead to satisfy the link.
#[cfg(not(test))]
extern "C" {
    fn __rt_gc_collect_cycles();
}

/// Invokes the runtime cycle collector. In non-test builds this calls the real
/// compiler-emitted `__rt_gc_collect_cycles`; in test builds it is a no-op stub
/// because the program's runtime object (which defines the symbol) is not linked
/// into the elephc-web rlib test binary.
#[inline]
fn rt_gc_collect_cycles() {
    #[cfg(not(test))]
    {
        // SAFETY: the runtime helper is compiler-emitted and reads only the
        // heap header table; it does not touch Rust stack beyond its own frame.
        unsafe { __rt_gc_collect_cycles(); }
    }
    #[cfg(test)]
    {
        rt_gc_collect_cycles_test_stub();
    }
}

/// Test-only no-op stand-in for the runtime cycle collector, present so the
/// elephc-web test binary links without the program's runtime object. Never
/// compiled into a real `--web` / `--web-worker` binary.
#[cfg(test)]
fn rt_gc_collect_cycles_test_stub() {}

// Worker-mode per-request reset routine provided by the compiled program's
// runtime. Releases and zeroes the previous request's superglobal hash tables
// and resets the concat-buffer offset, so the trampoline's fill functions see
// clean storage each request. The compiler always emits this helper under
// `--web-worker`, so the symbol resolves at link time. In `cfg(test)` the
// program's runtime object is absent, so a no-op local stub is used instead.
#[cfg(not(test))]
extern "C" {
    fn __rt_web_worker_request_reset();
}

/// Invokes the worker per-request reset. In non-test builds this calls the real
/// compiler-emitted `__rt_web_worker_request_reset`; in test builds it is a
/// no-op stub because the program's runtime object is not linked into the
/// elephc-web rlib test binary.
#[inline]
fn rt_web_worker_request_reset() {
    #[cfg(not(test))]
    {
        // SAFETY: the runtime helper is compiler-emitted and releases only the
        // request superglobal symbols and the concat offset; it does not touch
        // Rust stack beyond its own frame.
        unsafe { __rt_web_worker_request_reset(); }
    }
    #[cfg(test)]
    {
        rt_web_worker_request_reset_test_stub();
    }
}

/// Test-only no-op stand-in for the worker per-request reset, present so the
/// elephc-web test binary links without the program's runtime object. Never
/// compiled into a real `--web-worker` binary.
#[cfg(test)]
fn rt_web_worker_request_reset_test_stub() {}

/// Process-static worker config, set by the master in the forked child before
/// the boot runs. Accessed only through `addr_of_mut!` (never a `&mut` to a
/// `static mut`). `enter_worker_loop` consumes it via `take()`.
static mut WORKER_CONFIG: Option<WorkerConfig> = None;

/// Number of requests this worker has served, used by `--max-requests` recycling
/// and by the `--worker-gc-interval` cycle-collector cadence. Process-local
/// (each forked worker has its own copy starting at 0).
static SERVED: AtomicUsize = AtomicUsize::new(0);

/// Per-request handler time limit in seconds (`0` = none), read by `run_handler`
/// to arm a `SIGALRM` watchdog around the blocking handler call. Mirrors
/// `worker::MAX_EXEC_SECS` for the worker mode.
static MAX_EXEC_SECS: AtomicU32 = AtomicU32::new(0);

/// Process-static list of multipart temp file paths registered by the PHP prelude
/// via `elephc_web_register_tmp_file`. After each request, `enter_worker_loop`
/// unlinks them all and clears the list so worker-mode processes do not leak
/// temp files across requests. Accessed only through `addr_of_mut!`.
static mut TMP_FILES: Vec<CString> = Vec::new();

/// Minimum response size (bytes) worth gzip-compressing; mirrors `worker.rs`.
const GZIP_MIN_LEN: usize = 256;

/// Stores the worker config in the process-static slot. Called by
/// `server::spawn_worker` in the child process after fork and before the boot
/// function runs, so `enter_worker_loop` (reached from inside the boot) can
/// retrieve it. Writes through `addr_of_mut!`; never borrows the `static mut`.
pub(crate) fn set_worker_config(cfg: WorkerConfig) {
    // SAFETY: single-threaded per worker; written through a raw pointer.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(WORKER_CONFIG), Some(cfg));
    }
}

/// Takes the stored worker config, panicking if it was never set. Called at the
/// top of `enter_worker_loop`. Reads and clears the slot through `addr_of_mut!`.
fn take_worker_config() -> WorkerConfig {
    // SAFETY: single-threaded per worker; taken through a raw pointer.
    unsafe { (*core::ptr::addr_of_mut!(WORKER_CONFIG)).take() }
        .expect("worker config not set before enter_worker_loop")
}

/// Returns the stored worker config without consuming it, for tests. Reads the
/// slot through `addr_of_mut!`; never borrows the `static mut`.
#[cfg(test)]
pub(crate) fn peek_worker_config() -> Option<WorkerConfig> {
    // SAFETY: single-threaded per worker/test; read through a raw pointer.
    unsafe { *core::ptr::addr_of!(WORKER_CONFIG) }
}

/// Registers a multipart temp file path created by the PHP prelude so the worker
/// loop can `unlink` it after the request completes. Called by the compiled web
/// prelude (via the C-ABI `elephc_web_register_tmp_file` symbol) right after
/// `file_put_contents` writes a multipart upload to its tempnam path. The path is
/// copied into a `CString` and stored in the process-static `TMP_FILES` list.
///
/// # Safety
/// `path` must point to a NUL-terminated C string valid for the call. Single-
/// threaded per worker, so the list append cannot race.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_register_tmp_file(path: *const c_char) {
    if path.is_null() {
        return;
    }
    if let Ok(s) = std::ffi::CStr::from_ptr(path).to_str() {
        if let Ok(owned) = CString::new(s) {
            (*core::ptr::addr_of_mut!(TMP_FILES)).push(owned);
        }
    }
}

/// Unlinks every registered multipart temp file and clears the list. Called by
/// `enter_worker_loop` after each request's response is built, before the GC
/// cycle-collection call. Failures to unlink are logged to stderr but do not
/// abort the worker: a missing file is benign (the handler may have removed it
/// via `unlink`, or the request had no multipart uploads).
fn cleanup_tmp_files() {
    // SAFETY: single-threaded per worker; the list is drained through a raw
    // pointer, never forming a reference to the `static mut`.
    let files = unsafe { core::mem::take(&mut *core::ptr::addr_of_mut!(TMP_FILES)) };
    for path in files {
        // SAFETY: CString as_ptr is a valid NUL-terminated pointer for unlink.
        unsafe { libc::unlink(path.as_ptr()); }
    }
}

/// Runs the runtime cycle collector to reclaim cyclic garbage, gated by the
/// `--worker-gc-interval` cadence. `0` disables collection entirely; `1` collects
/// every request; `N` collects every N-th request. Called by `enter_worker_loop`
/// after each handler invocation, once the response state has been drained.
fn maybe_collect_cycles(gc_interval: u32) {
    if gc_interval == 0 {
        return;
    }
    let served = SERVED.load(Ordering::Relaxed);
    if served % gc_interval as usize == 0 {
        rt_gc_collect_cycles();
    }
}

/// `SIGALRM` handler: a worker-mode handler that ran past `--max-execution-time`
/// is killed so the master respawns the worker. Async-signal-safe: only
/// `write` + `_exit`. Mirrors `worker::handle_exec_timeout`.
extern "C" fn handle_exec_timeout(_sig: libc::c_int) {
    const MSG: &[u8] =
        b"elephc-web: worker handler exceeded --max-execution-time; recycling worker\n";
    unsafe {
        libc::write(2, MSG.as_ptr() as *const libc::c_void, MSG.len());
        libc::_exit(1);
    }
}

/// Installs the `SIGALRM` execution-timeout handler in this worker. Mirrors
/// `worker::install_exec_timeout_handler`.
fn install_exec_timeout_handler() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handle_exec_timeout as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGALRM, &sa, std::ptr::null_mut());
    }
}

/// Entry point of the worker loop, reached from `elephc_web_worker_register`.
/// Consumes the worker config, builds the SO_REUSEPORT listener and a tokio
/// current-thread runtime + LocalSet, then loops: accept → parse → set_request
/// → reset response → invoke handler trampoline → take response → unlink temp
/// files → GC → send. Exits the process on `--max-requests` exhaustion or fatal
/// error. Never returns.
pub(crate) fn enter_worker_loop() -> ! {
    let cfg = take_worker_config();
    let WorkerConfig {
        max_body,
        max_requests,
        access_log,
        max_exec_secs,
        gzip,
        worker_gc_interval,
        // Read straight off `cfg` (still valid: `WorkerConfig` is `Copy`) in the
        // close predicate; only the idle timeout needs a loop-invariant local.
        max_conn_requests: _,
        idle_timeout_secs,
        handler_offload,
        max_pending,
    } = cfg;
    if max_exec_secs > 0 {
        MAX_EXEC_SECS.store(max_exec_secs, Ordering::Relaxed);
        install_exec_timeout_handler();
    }
    // The listen address is carried inside WorkerConfig? No: WorkerConfig holds
    // per-request limits only; the listen addr is parsed by the master and must
    // be available here. We carry it through the same static used by the classic
    // mode is not possible (it is passed by arg). For worker mode, the master
    // sets the listen address via set_worker_listen before boot.
    let listen = take_worker_listen();
    let listen_addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("elephc-web: invalid --listen address {:?}", listen);
            std::process::exit(1);
        }
    };
    // Master dispatch (`--dispatch master`) installs the child socketpair end into
    // a process-static slot before boot; kernel dispatch leaves it unset. Take it
    // now so the loop uses `ConnSource::Master` (receive fds, do NOT bind) or
    // `ConnSource::Kernel` (bind a SO_REUSEPORT listener, unchanged behavior).
    let child_chan = crate::dispatch::take_child_dispatch_chan();
    // Handler offload: spawn the dedicated `php-handler` thread + bounded job queue
    // when enabled. Boot has already completed (this loop is reached from inside
    // register), so the spawn's happens-before publishes the boot's heap/globals to
    // the handler thread. SIGALRM is blocked on THIS thread first so the
    // `--max-execution-time` alarm (armed on the handler thread inside
    // `run_worker_handler`) is delivered there deterministically. Each job runs the
    // whole worker-mode per-request body via `run_one_worker_job`.
    let offload_tx = if handler_offload {
        offload::block_sigalrm_on_io_thread();
        let (tx, rx) = tokio::sync::mpsc::channel::<RequestJob>(max_pending);
        offload::spawn_handler_thread(rx, move |job| {
            run_one_worker_job(job, worker_gc_interval);
        });
        Some(tx)
    } else {
        None
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("elephc-web: tokio runtime build failed: {}", e);
            std::process::exit(1);
        });
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        // Build the connection source: master mode wraps the socketpair end; kernel
        // mode binds the SO_REUSEPORT listener (unchanged behavior).
        let conn_source = match child_chan {
            Some(chan_fd) => match ConnSource::master(chan_fd) {
                Ok(cs) => cs,
                Err(e) => {
                    eprintln!("elephc-web: failed to set up master dispatch channel: {}", e);
                    std::process::exit(1);
                }
            },
            None => {
                let std_listener = match reuseport_listener(listen_addr) {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("elephc-web: failed to bind {}: {}", listen_addr, e);
                        std::process::exit(1);
                    }
                };
                match TcpListener::from_std(std_listener) {
                    Ok(l) => ConnSource::Kernel(l),
                    Err(e) => {
                        eprintln!("elephc-web: failed to register listener: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        };
        // Idle-watchdog duration, computed once (used only when `--idle-timeout` > 0).
        let idle = Duration::from_secs(idle_timeout_secs as u64);
        // Connection-serving config is identical for every connection, so build the
        // hyper HTTP/1 builder once and reuse it (serve_connection takes &self).
        // `header_read_timeout(30s)` is kept for anti-slowloris; the configurable
        // idle rotation lives in the `--idle-timeout` watchdog. See the WI-4/Q4
        // note in `crate::worker::serve` for hyper 1.10.1's idle semantics.
        let mut http = http1::Builder::new();
        http.timer(TokioTimer::new())
            .header_read_timeout(Duration::from_secs(30));
        loop {
            // --max-requests recycling. In master mode this runs BEFORE `next()`
            // sends READY (cap-before-READY), so a capped worker exits without being
            // handed one more connection.
            if max_requests > 0 && SERVED.load(Ordering::Relaxed) >= max_requests {
                break;
            }
            // Next connection: kernel `accept()` (peer from accept, addr = listen
            // addr) or master READY→recv_fd (peer/addr via getpeername/getsockname).
            let (stream, peer, addr) = match conn_source.next(listen_addr).await {
                NextConn::Serve(s, p, a) => (s, p, a),
                NextConn::Retry => continue,
                NextConn::Closed => break, // master gone → exit cleanly below
            };
            // Disable Nagle: worker-mode responses are small and written in one
            // shot, so Nagle/delayed-ACK interaction would add latency to
            // keep-alive round-trips. Best-effort (matches classic --web).
            let _ = stream.set_nodelay(true);
            // TLS: read the process-wide acceptor (built pre-fork; `None` on
            // plaintext). The handshake runs INSIDE the connection task below, never
            // in this accept loop, so a slow client handshake cannot stall accepting
            // others. `https` is threaded into the request path so PHP sees
            // `$_SERVER['HTTPS']`.
            let acceptor = crate::tls::tls_acceptor();
            let https = acceptor.is_some();
            // Per-connection keep-alive rotation state (see `crate::worker::serve`),
            // allocated ONLY when the relevant feature is enabled so the default
            // (both off) hot path keeps the original zero-allocation, zero-bookkeeping
            // behavior. `rotate_on` gates the response counter + close/C3-drain check;
            // `idle_on` gates the last-activity stamps + idle watchdog. When on, each
            // cell lives in this connection's !Send task.
            let rotate_on = cfg.max_conn_requests > 0 || cfg.max_requests > 0;
            let idle_on = idle_timeout_secs > 0;
            let conn_served = rotate_on.then(|| Rc::new(Cell::new(0usize)));
            let last_activity = idle_on.then(|| Rc::new(Cell::new(Instant::now())));
            let watchdog_activity = last_activity.clone();
            // Per-connection clone of the offload sender (cloning `None` is free);
            // each request further clones it into its own `async move`.
            let conn_offload_tx = offload_tx.clone();
            // `service_fn` is FnMut — called once per request on this connection —
            // so the OUTER closure is non-async and clones the per-connection
            // `Option<Rc<..>>` handles into each returned `async move` block (cloning
            // `None` is free); the Copy config values are copied in.
            let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                let conn_served = conn_served.clone();
                let last_activity = last_activity.clone();
                let offload_tx = conn_offload_tx.clone();
                async move {
                    // A request just arrived on this connection: stamp activity so
                    // the idle watchdog measures inactivity from now (only when the
                    // idle timeout is enabled; otherwise the handle is `None`).
                    if let Some(la) = &last_activity {
                        la.set(Instant::now());
                    }
                    let started = Instant::now();
                    let method = req.method().as_str().to_string();
                    let uri = req.uri().to_string();
                    let path = req.uri().path().to_string();
                    let query = req.uri().query().unwrap_or("").to_string();
                    let protocol = crate::worker::version_str(req.version());
                    let log_method_path = if access_log { Some((method.clone(), path.clone())) } else { None };
                    let accepts_gzip = gzip
                        && req.headers().get(hyper::header::ACCEPT_ENCODING).is_some_and(|v| {
                            v.to_str().map(|s| s.to_ascii_lowercase().contains("gzip")).unwrap_or(false)
                        });
                    // Collect headers straight into their final (name, value, php_name)
                    // CString form, so no intermediate owned (String, String) copy is
                    // made per request and the $_SERVER key is precomputed in Rust.
                    let headers: Vec<(CString, CString, CString)> = req
                        .headers()
                        .iter()
                        .map(|(n, v)| request_state::request_header_cstrings(n.as_str(), v.as_bytes()))
                        .collect();
                    // Keep the collected body as `Bytes` (no `.to_vec()` copy): it is
                    // stored directly and exposed to PHP by pointer.
                    let collected = if max_body == 0 {
                        req.into_body().collect().await.map(|c| c.to_bytes()).map_err(|_| ())
                    } else {
                        Limited::new(req.into_body(), max_body)
                            .collect()
                            .await
                            .map(|c| c.to_bytes())
                            .map_err(|_| ())
                    };
                    let body = match collected {
                        Ok(b) => b,
                        Err(_) => {
                            let resp = Response::builder()
                                .status(413)
                                .body(Full::new(Bytes::from_static(b"Payload Too Large")))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))));
                            return Ok::<_, Infallible>(resp);
                        }
                    };
                    let meta = request_state::RequestMeta {
                        remote_addr: peer.ip().to_string(),
                        remote_port: peer.port(),
                        server_addr: addr.ip().to_string(),
                        server_port: addr.port(),
                        protocol,
                        https,
                    };
                    // Produce the response triple inline (I/O thread) or offloaded
                    // (php-handler thread). The inline branch is byte-for-byte
                    // today's path; the offload branch touches NO PHP state here —
                    // the whole per-request body runs in `run_one_worker_job`.
                    let (status, resp_headers, resp_body) = match &offload_tx {
                        Some(tx) => {
                            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                            let job = RequestJob {
                                method,
                                uri,
                                path,
                                query,
                                headers,
                                body,
                                meta,
                                reply: reply_tx,
                            };
                            if tx.try_send(job).is_err() {
                                return Ok::<_, Infallible>(offload::queue_full_response());
                            }
                            match reply_rx.await {
                                Ok(parts) => (parts.status, parts.headers, parts.body),
                                Err(_) => {
                                    return Ok::<_, Infallible>(offload::handler_gone_response());
                                }
                            }
                        }
                        None => {
                            request_state::set_request(method, uri, path, query, headers, body, meta);
                            let resp_body = run_worker_handler();
                            // Unlink multipart temp files the PHP prelude registered for
                            // this request, then run the cycle collector per the
                            // --worker-gc-interval cadence. Temp-file cleanup happens
                            // before GC so the collector sees the freed file-scope
                            // arrays before walking the heap.
                            cleanup_tmp_files();
                            maybe_collect_cycles(worker_gc_interval);
                            let status = request_state::take_status();
                            let resp_headers = request_state::take_headers();
                            (status, resp_headers, resp_body)
                        }
                    };
                    let already_encoded = resp_headers
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("content-encoding"));
                    let gzipped = if accepts_gzip && !already_encoded && resp_body.len() >= GZIP_MIN_LEN {
                        crate::worker::gzip_bytes(&resp_body)
                    } else {
                        None
                    };
                    let do_gzip = gzipped.is_some();
                    let resp_body = gzipped.unwrap_or(resp_body);
                    let mut builder = Response::builder().status(status);
                    for (name, value) in resp_headers {
                        builder = builder.header(name, value);
                    }
                    if do_gzip {
                        builder = builder.header("content-encoding", "gzip");
                    }
                    // Keep-alive rotation (only when a rotation feature is enabled):
                    // count this response, then close the connection
                    // (`Connection: close`) when this connection hit its per-connection
                    // cap OR the worker hit its `--max-requests` recycle cap (the C3
                    // drain). Uses this module's own `SERVED`. With both features off
                    // this block is skipped entirely (`conn_served` is `None`).
                    if rotate_on {
                        let served = conn_served
                            .as_ref()
                            .map(|c| {
                                c.set(c.get() + 1);
                                c.get()
                            })
                            .unwrap_or(0);
                        if should_close_connection(served, SERVED.load(Ordering::Relaxed), &cfg) {
                            builder = builder.header(hyper::header::CONNECTION, "close");
                        }
                    }
                    // Response produced: stamp activity again (only when the idle
                    // timeout is enabled) so an idle wait for the next request is
                    // measured from the response, not the arrival.
                    if let Some(la) = &last_activity {
                        la.set(Instant::now());
                    }
                    let response = builder
                        .body(Full::new(Bytes::from(resp_body)))
                        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))));
                    if let Some((m, p)) = log_method_path {
                        eprintln!(
                            "{} \"{} {}\" {} {}ms",
                            peer.ip(),
                            m,
                            p,
                            status,
                            started.elapsed().as_millis()
                        );
                    }
                    Ok::<_, Infallible>(response)
                }
            });
            // Clone the hyper builder so the connection future OWNS it (the handshake
            // and `serve_connection` run after the stream is accepted, so the future
            // cannot borrow the loop-local `http`). The builder is a small config
            // struct, so the per-connection clone is cheap.
            let http = http.clone();
            // Drive the whole connection lifecycle (TLS handshake + serve + PR1 idle
            // watchdog) via the shared helper. Kernel mode spawns it so connections
            // interleave concurrently, exactly as before; master mode (slot = 1)
            // awaits it inline so the worker serves one connection at a time and only
            // sends the next READY once this connection's `serve_connection`
            // completes.
            if conn_source.is_serial() {
                drive_connection(
                    stream, peer, acceptor, http, service, watchdog_activity, idle, access_log,
                )
                .await;
            } else {
                tokio::task::spawn_local(drive_connection(
                    stream, peer, acceptor, http, service, watchdog_activity, idle, access_log,
                ));
            }
        }
    });
    std::process::exit(0);
}

/// Process-static listen address for the worker loop, set by the master in the
/// forked child before boot. `WorkerConfig` carries only per-request limits, so
/// the listen address travels through this separate slot.
static mut WORKER_LISTEN: Option<String> = None;

/// Stores the listen address in the process-static slot. Called by the master
/// in the forked child before boot. Writes through `addr_of_mut!`.
pub(crate) fn set_worker_listen(listen: String) {
    // SAFETY: single-threaded per worker; written through a raw pointer.
    unsafe {
        core::ptr::write(core::ptr::addr_of_mut!(WORKER_LISTEN), Some(listen));
    }
}

/// Takes the stored listen address, panicking if it was never set. Reads and
/// clears the slot through `addr_of_mut!`.
fn take_worker_listen() -> String {
    // SAFETY: single-threaded per worker; taken through a raw pointer.
    unsafe { (*core::ptr::addr_of_mut!(WORKER_LISTEN)).take() }
        .expect("worker listen address not set before enter_worker_loop")
}

/// Invokes the registered request handler for one request and returns the
/// captured response body. Mirrors `worker::run_handler` but calls the opaque
/// handler stored by `handler` instead of the classic per-request re-executed
/// top-level PHP.
///
/// Two handler-mode shapes share this loop: `--web-worker` registers a
/// `c_int`-returning `WorkerHandler` trampoline via `elephc_web_worker_register`,
/// while `--web-worker=script` registers a void `ScriptHandler` (the compiled
/// top-level body itself) via `register_script_handler`. The script handler is
/// preferred when present so the correct ABI is used — the two are never both
/// registered in the same process.
fn run_worker_handler() -> Vec<u8> {
    request_state::set_capture(true);
    request_state::clear_body();
    request_state::reset_response();
    // Arm the execution-timeout watchdog around the blocking handler, if enabled.
    let secs = MAX_EXEC_SECS.load(Ordering::Relaxed);
    if secs > 0 {
        unsafe { libc::alarm(secs); }
    }
    // SAFETY: the handler trampoline/body is compiler-emitted and reads/writes
    // only the request/response process-statics; it does not access Rust stack
    // beyond its own frame.
    rt_web_worker_request_reset();
    if let Some(h) = handler::script_handler() {
        unsafe { h(); }
    } else {
        let h = handler::worker_handler()
            .expect("worker handler not registered before request");
        let _rc = unsafe { h() };
    }
    if secs > 0 {
        unsafe { libc::alarm(0); }
    }
    SERVED.fetch_add(1, Ordering::Relaxed);
    request_state::take_body()
}

/// Runs the FULL worker-mode per-request body for one offloaded job on the
/// dedicated `php-handler` thread and replies with the produced `ResponseParts`.
/// This is the exact inline sequence, moved verbatim: `set_request` →
/// `run_worker_handler` (reset + dispatch + `--max-execution-time` watchdog +
/// `SERVED`++ + `take_body`) → `cleanup_tmp_files` → `maybe_collect_cycles` →
/// drain status/headers. Every step touches handler-thread-affine PHP state, so it
/// all runs here. A dropped receiver (client gone) makes `reply.send` a no-op.
/// Factored as a free function so the ZTS (N>1) work can wrap it in a lock.
/// `gc_interval` is the `--worker-gc-interval` cadence.
fn run_one_worker_job(job: RequestJob, gc_interval: u32) {
    let RequestJob {
        method,
        uri,
        path,
        query,
        headers,
        body,
        meta,
        reply,
    } = job;
    request_state::set_request(method, uri, path, query, headers, body, meta);
    let resp_body = run_worker_handler();
    cleanup_tmp_files();
    maybe_collect_cycles(gc_interval);
    let status = request_state::take_status();
    let headers = request_state::take_headers();
    let _ = reply.send(ResponseParts {
        status,
        headers,
        body: resp_body,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Purpose:
    // Unit tests for worker-mode config/listen storage and the read helpers.
    //
    // Called from:
    // - `cargo test` through Rust's test harness.
    //
    // Key details:
    // - These tests mutate process-static state; they run single-threaded and
    //   do not invoke `enter_worker_loop` (which diverges via process exit).

    /// Verifies `set_worker_config`/`peek_worker_config` round-trip the config.
    #[test]
    fn worker_config_roundtrip() {
        let cfg = WorkerConfig {
            max_body: 1024,
            max_requests: 5,
            access_log: true,
            max_exec_secs: 0,
            gzip: false,
            worker_gc_interval: 1,
            max_conn_requests: 250,
            idle_timeout_secs: 45,
            handler_offload: true,
            max_pending: 16,
        };
        set_worker_config(cfg);
        let peeked = peek_worker_config();
        assert!(peeked.is_some());
        let peeked = peeked.unwrap();
        assert_eq!(peeked.max_body, 1024);
        assert_eq!(peeked.max_conn_requests, 250);
        assert_eq!(peeked.idle_timeout_secs, 45);
    }

    /// Verifies `set_worker_listen` then `take_worker_listen` returns the value.
    /// We cannot call `take_worker_listen` if it was already consumed by a prior
    /// test path, so this test sets a fresh value first.
    #[test]
    fn worker_listen_roundtrip() {
        set_worker_listen("127.0.0.1:0".to_string());
        // SAFETY: single-threaded test; take through a raw pointer.
        let taken = unsafe { (*core::ptr::addr_of_mut!(WORKER_LISTEN)).take() };
        assert_eq!(taken.as_deref(), Some("127.0.0.1:0"));
    }

    /// Verifies `elephc_web_register_tmp_file` appends a path to the static list
    /// and `cleanup_tmp_files` drains it. Registers real paths that do not exist
    /// so `unlink` is a no-op (ENOENT is ignored); the cleanup must still clear
    /// the list regardless.
    #[test]
    fn tmp_file_register_and_cleanup() {
        use std::ffi::CString;
        // SAFETY: single-threaded test; clear the static list first so we observe
        // a clean before/after.
        unsafe {
            (*core::ptr::addr_of_mut!(TMP_FILES)).clear();
        }
        let p = CString::new("/tmp/elephc_test_does_not_exist_1").unwrap();
        unsafe { elephc_web_register_tmp_file(p.as_ptr()); }
        let q = CString::new("/tmp/elephc_test_does_not_exist_2").unwrap();
        unsafe { elephc_web_register_tmp_file(q.as_ptr()); }
        // SAFETY: single-threaded test; read through a raw pointer.
        let count = unsafe { (*core::ptr::addr_of_mut!(TMP_FILES)).len() };
        assert_eq!(count, 2);
        cleanup_tmp_files();
        // SAFETY: single-threaded test; read through a raw pointer.
        let after = unsafe { (*core::ptr::addr_of_mut!(TMP_FILES)).len() };
        assert_eq!(after, 0);
    }

    /// Verifies `maybe_collect_cycles` does not panic for interval 0 and 1. In
    /// test builds the real `__rt_gc_collect_cycles` is replaced by a no-op
    /// stub (the program's runtime object is not linked), so this only checks
    /// that the gating logic and the stub dispatch path are sound.
    #[test]
    fn gc_interval_gating_no_panic() {
        maybe_collect_cycles(0);
        maybe_collect_cycles(1);
        maybe_collect_cycles(7);
    }
}