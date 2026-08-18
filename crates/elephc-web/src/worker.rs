//! Purpose:
//! Per-worker HTTP serving: build a SO_REUSEPORT listening socket, run a tokio
//! current-thread runtime, and dispatch each request to the PHP handler.
//!
//! Called from:
//! - `crate::server::elephc_web_run` in each forked child process.
//!
//! Key details:
//! - current-thread runtime + a blocking handler() call means PHP never runs on
//!   two threads in one worker; concurrency comes from the N forked workers.
//! - SO_REUSEPORT lets every worker bind the same port; the kernel balances.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::request_state;
use crate::session::upload_progress;

/// Pending-connection backlog for each worker's listening socket.
const LISTEN_BACKLOG: i32 = 1024;

/// Builds a listening std::net::TcpListener with SO_REUSEPORT set, bound to `addr`.
fn reuseport_listener(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    sock.listen(LISTEN_BACKLOG)?;
    Ok(sock.into())
}

/// Number of requests this worker has served, used by `--max-requests` recycling.
/// Process-local (each forked worker has its own copy starting at 0).
static SERVED: AtomicUsize = AtomicUsize::new(0);

/// Records one completed handler and broadcasts a graceful recycle at the quota.
fn record_completed_request(max_requests: usize, recycle: &watch::Sender<bool>) {
    let served = SERVED.fetch_add(1, Ordering::Relaxed) + 1;
    if max_requests > 0 && served >= max_requests {
        let _ = recycle.send(true);
    }
}

/// Exit code a worker child uses for a planned `--max-requests` recycle.
/// Distinct from 0 (clean exit), 1 (worker setup/handler errors), and 2 (usage
/// errors) so the master can tell an intentional recycle from a genuine crash:
/// under sustained traffic a worker can serve its whole quota in well under the
/// master's fast-death window, and counting that as a crash-on-startup would
/// shut the server down. Checked by `server::is_planned_recycle`.
pub(crate) const RECYCLE_EXIT_CODE: i32 = 86;

/// Per-request handler time limit in seconds (`0` = none), read by `run_handler`
/// to arm a `SIGALRM` watchdog around the blocking `handler()` call.
static MAX_EXEC_SECS: AtomicU32 = AtomicU32::new(0);

/// `SIGALRM` handler: a handler that ran past `--max-execution-time` is killed so
/// the master respawns the worker (a runaway handler would otherwise pin the
/// single worker thread forever). Async-signal-safe: only `write` + `_exit`.
extern "C" fn handle_exec_timeout(_sig: libc::c_int) {
    const MSG: &[u8] = b"elephc-web: handler exceeded --max-execution-time; recycling worker\n";
    unsafe {
        libc::write(2, MSG.as_ptr() as *const libc::c_void, MSG.len());
        libc::_exit(1);
    }
}

/// Installs the `SIGALRM` execution-timeout handler in this worker.
fn install_exec_timeout_handler() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handle_exec_timeout as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGALRM, &sa, std::ptr::null_mut());
    }
}

/// Per-worker serving configuration (all `Copy`, so it survives `fork` and moves
/// into the connection tasks freely).
#[derive(Clone, Copy)]
pub struct WorkerConfig {
    /// Max request body in bytes; `0` = unlimited (over-limit → HTTP 413).
    pub max_body: usize,
    /// Recycle the worker after this many requests; `0` = never.
    pub max_requests: usize,
    /// Log one line per request to stderr.
    pub access_log: bool,
    /// Per-request handler time limit in seconds; `0` = no limit.
    pub max_exec_secs: u32,
    /// gzip the response body when the client sent `Accept-Encoding: gzip`.
    pub gzip: bool,
}

/// Minimum response size (bytes) worth gzip-compressing; below this the framing
/// overhead outweighs the savings.
const GZIP_MIN_LEN: usize = 256;

/// Serves HTTP on `listen` (host:port) in this worker process. Builds a
/// current-thread tokio runtime and loops accepting connections, serving each
/// with the PHP handler per `WorkerConfig`.
pub fn serve(listen: &str, handler: extern "C" fn(), cfg: WorkerConfig) {
    let WorkerConfig {
        max_body,
        max_requests,
        access_log,
        max_exec_secs,
        gzip,
    } = cfg;
    if max_exec_secs > 0 {
        MAX_EXEC_SECS.store(max_exec_secs, Ordering::Relaxed);
        install_exec_timeout_handler();
    }
    // Re-arm the sampling probe in this worker: the fork disarmed the inherited
    // timer (so exec'd children cannot die from the default SIGPROF action), and
    // a worker keeps running elephc code, so it must sample again. No-op unless
    // the binary was built --probe.
    probe_route::rearm();
    let addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("elephc-web: invalid --listen address {:?}", listen);
            std::process::exit(1);
        }
    };
    let std_listener = match reuseport_listener(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("elephc-web: failed to bind {}: {}", addr, e);
            std::process::exit(1);
        }
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    // A LocalSet lets each connection run as its own !Send task on this single
    // thread, so a slow or idle keep-alive connection does not block the accept
    // loop from taking new connections. The blocking handler() call is synchronous
    // (no await), so PHP execution still serializes on the one worker thread —
    // only the async request/response I/O of different connections interleaves.
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let listener = match TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("elephc-web: failed to register listener: {}", e);
                std::process::exit(1);
            }
        };
        let (recycle, mut recycle_requested) = watch::channel(false);
        let mut connections = Vec::new();
        loop {
            connections.retain(|connection: &tokio::task::JoinHandle<_>| {
                !connection.is_finished()
            });
            // --max-requests recycling: stop accepting once the cap is reached so
            // the master respawns a fresh worker (bounds memory growth over time).
            if *recycle_requested.borrow() {
                break;
            }
            let accepted = tokio::select! {
                changed = recycle_requested.changed() => {
                    if changed.is_err() || *recycle_requested.borrow() {
                        break;
                    }
                    continue;
                }
                accepted = listener.accept() => accepted,
            };
            let (stream, peer) = match accepted {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            let request_recycle = recycle.clone();
            let mut connection_recycle = recycle.subscribe();
            connections.push(tokio::task::spawn_local(async move {
                let connection = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(30))
                    .serve_connection(io, service_fn(move |req: Request<hyper::body::Incoming>| {
                        let request_recycle = request_recycle.clone();
                        async move {
                    // Seed session deployment config before upload-progress body
                    // draining. The PHP prelude repeats this reset immediately
                    // before the handler, so both phases see identical values and
                    // no request can inherit the prior request's session settings.
                    unsafe { crate::session::elephc_web_session_reset() };
                    let started = Instant::now();
                    let method = req.method().as_str().to_string();
                    let uri = req.uri().to_string();
                    let path = req.uri().path().to_string();
                    let query = req.uri().query().unwrap_or("").to_string();
                    let protocol = format!("{:?}", req.version());
                    // Captured for the optional access log (method/path are moved into set_request).
                    let log_method_path = if access_log { Some((method.clone(), path.clone())) } else { None };
                    // The route label the sampling probe stamps onto samples taken
                    // during this request (no-op unless the binary was built --probe).
                    let probe_route = format!("{method} {path}");
                    let accepts_gzip = gzip
                        && req.headers().get(hyper::header::ACCEPT_ENCODING).is_some_and(|v| {
                            v.to_str().map(|s| s.to_ascii_lowercase().contains("gzip")).unwrap_or(false)
                        });
                    let headers: Vec<(String, String)> = req
                        .headers()
                        .iter()
                        .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
                        .collect();
                    // The body must be fully collected (async) BEFORE the blocking handler
                    // runs, since handler() cannot yield on the current-thread runtime.
                    // Collect with a size cap (0 = unlimited); an over-limit body
                    // short-circuits to 413 without ever running the PHP handler.
                    //
                    // For a tracked `session.upload_progress` multipart upload, the body
                    // is drained frame-by-frame so progress can be written to the session
                    // file as bytes arrive, while still buffering the complete body the
                    // handler sees. Non-tracked requests keep the simple fast path.
                    let collected = if let Some(mut tracker) = upload_progress::begin(&headers, &query) {
                        drain_with_progress(req.into_body(), max_body, &mut tracker).await
                    } else if max_body == 0 {
                        req.into_body().collect().await.map(|c| c.to_bytes().to_vec()).map_err(|_| ())
                    } else {
                        Limited::new(req.into_body(), max_body)
                            .collect()
                            .await
                            .map(|c| c.to_bytes().to_vec())
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
                    };
                    // Capture the request Cookie/Host before `headers` is moved into
                    // set_request; they gate/steer optional `session.use_trans_sid`
                    // URL rewriting (Cookie → is the SID already client-held?, Host →
                    // same-origin matching). The rewrite itself stays gated by session
                    // config, so the default path only pays these two small lookups.
                    let req_cookie = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case("cookie"))
                        .map(|(_, v)| v.clone());
                    let req_host = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case("host"))
                        .map(|(_, v)| v.clone());
                    // Distributed profiling: continue the caller's trace when it
                    // sent a W3C `traceparent`, else start one. Read before
                    // `headers` is moved into set_request, like Cookie/Host.
                    let req_traceparent = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case("traceparent"))
                        .map(|(_, v)| v.clone());
                    // Opt in per request, the way a profiler you can leave in
                    // production has to work: the caller asks, and only that
                    // request pays. Absent the header the hooks stay as the
                    // environment left them, so `--web --instrument` without
                    // `ELEPHC_INSTR_OFF` keeps profiling every request.
                    // Signed, not merely present: the value is
                    // `t=<unix seconds>,v=<hmac>` over the build key, so a header
                    // captured from a log stops working within minutes and one
                    // invented from scratch never does.
                    let profile_this = headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case("x-elephc-query"))
                        .is_some_and(|(_, value)| probe_route::query_is_authentic(value));
                    request_state::set_request(method, uri, path, query, headers, body, meta);
                    probe_route::trace_begin(req_traceparent.as_deref(), &probe_route);
                    probe_route::set(&probe_route);
                    if profile_this {
                        probe_route::profile_request(true);
                    }
                    let resp_body = run_handler(handler);
                    if profile_this {
                        // Ends and dumps this request's slice, so consecutive
                        // profiled requests stay separate captures.
                        probe_route::profile_request(false);
                    }
                    probe_route::clear();
                    let status = request_state::take_status();
                    let mut resp_headers = request_state::take_headers();
                    // Propagate the session id through same-origin URLs / forms when
                    // `session.use_trans_sid` is active (no-op otherwise). Runs on the
                    // plaintext body, before the gzip decision below.
                    let resp_body = crate::trans_sid::maybe_rewrite_response(
                        req_cookie.as_deref(),
                        req_host.as_deref(),
                        &mut resp_headers,
                        resp_body,
                    );
                    // gzip the body when the client accepts it, the body is large
                    // enough to be worth it, and the handler did not already set a
                    // Content-Encoding.
                    let already_encoded = resp_headers
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("content-encoding"));
                    let gzipped = if accepts_gzip && !already_encoded && resp_body.len() >= GZIP_MIN_LEN {
                        let compressed = gzip_bytes(&resp_body);
                        if compressed.is_some() {
                            resp_headers.retain(|(name, _)| {
                                !name.eq_ignore_ascii_case("content-length")
                            });
                        }
                        compressed
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
                    record_completed_request(max_requests, &request_recycle);
                    Ok::<_, Infallible>(response)
                }}));
                tokio::pin!(connection);
                tokio::select! {
                    _ = &mut connection => {}
                    changed = connection_recycle.changed() => {
                        if changed.is_ok() && *connection_recycle.borrow() {
                            connection.as_mut().graceful_shutdown();
                            let _ = connection.await;
                        }
                    }
                }
            }));
        }
        drop(listener);
        for connection in connections {
            let _ = connection.await;
        }
    });
}

/// gzip-compresses `data`, returning the compressed bytes, or `None` if encoding
/// failed (so the caller leaves the body uncompressed and sets no Content-Encoding).
fn gzip_bytes(data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).ok()?;
    encoder.finish().ok()
}

/// Drains the request body frame-by-frame while feeding an upload-progress
/// `Tracker`, returning the fully buffered body (byte-identical to what the
/// simple `collect()` path would produce). Each received data frame is appended
/// to the buffer and passed to `tracker.update`, which writes throttled progress
/// snapshots into the session file. The existing `max_body` cap is preserved:
/// an over-limit body returns `Err(())` so the caller short-circuits to 413
/// without running the handler. Once the body is fully drained, `tracker.complete`
/// performs the final progress write (or cleanup removal).
async fn drain_with_progress(
    mut body: hyper::body::Incoming,
    max_body: usize,
    tracker: &mut upload_progress::Tracker,
) -> Result<Vec<u8>, ()> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| ())?;
        if let Ok(data) = frame.into_data() {
            buf.extend_from_slice(&data);
            if max_body > 0 && buf.len() > max_body {
                return Err(());
            }
            tracker.update(&buf);
        }
    }
    tracker.complete(&buf);
    Ok(buf)
}

/// Runs the PHP handler for one request and returns the captured response body.
/// Optional bridge to the sampling probe's route tagging. The compiled program's
/// core runtime always defines the `_elephc_probe_route_fn` pointer slot (a `.comm`,
/// zero by default); under `--probe` the compiler stores `elephc_probe_set_route`
/// into it. Reading the slot and calling through it when non-null keeps route
/// tagging pay-for-use — a non-probe binary leaves the slot zero — with no dlsym
/// and no compile-time coupling to the probe crate.
mod probe_route {
    extern "C" {
        /// Runtime `.comm` slot holding `elephc_probe_set_route` under `--probe`,
        /// else zero. Mangled per target like the other runtime externs.
        static elephc_probe_route_fn: usize;
        /// Runtime `.comm` slot holding `elephc_probe_rearm` under `--probe`, else
        /// zero — the worker re-arm the fork disarmed.
        static elephc_probe_rearm_fn: usize;
        /// Runtime `.comm` slot holding `elephc_instr_trace_begin` under
        /// `--instrument`, else zero — opens this request's W3C trace context.
        static elephc_instr_trace_fn: usize;
        /// Runtime `.comm` slot holding `elephc_instr_request` under
        /// `--instrument`, else zero — brackets one request's own profile.
        static elephc_instr_request_fn: usize;
        /// Runtime `.comm` slot holding `elephc_probe_verify_query`, else zero —
        /// checks that a profiling request is signed by the build key.
        static elephc_probe_verify_fn: usize;
    }

    type SetRouteFn = unsafe extern "C" fn(*const u8, usize);
    type RearmFn = unsafe extern "C" fn();
    type TraceFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize);
    type RequestFn = unsafe extern "C" fn(u32);
    type VerifyFn = unsafe extern "C" fn(*const u8, usize) -> u32;

    fn resolve() -> Option<SetRouteFn> {
        let addr = unsafe { std::ptr::addr_of!(elephc_probe_route_fn).read() };
        if addr == 0 {
            None
        } else {
            Some(unsafe { std::mem::transmute::<usize, SetRouteFn>(addr) })
        }
    }

    /// Re-arms the probe timer in this worker, if the probe is linked.
    pub fn rearm() {
        let addr = unsafe { std::ptr::addr_of!(elephc_probe_rearm_fn).read() };
        if addr != 0 {
            let rearm = unsafe { std::mem::transmute::<usize, RearmFn>(addr) };
            unsafe { rearm() };
        }
    }

    /// Stamps `route` onto samples taken until `clear`.
    pub fn set(route: &str) {
        if let Some(set_route) = resolve() {
            unsafe { set_route(route.as_ptr(), route.len()) };
        }
    }

    /// Clears the active route so idle-worker samples are untagged.
    pub fn clear() {
        if let Some(set_route) = resolve() {
            unsafe { set_route(std::ptr::null(), 0) };
        }
    }

    /// Whether a profiling request carries a signature this binary accepts.
    ///
    /// Profiling costs the request real time and reveals the shape of the code,
    /// so asking for it is privileged: only a holder of the build key can. An
    /// unsigned trigger would let anyone who can set a header profile production.
    /// A binary without the tooling answers no, which is also the right answer.
    pub fn query_is_authentic(value: &str) -> bool {
        let addr = unsafe { std::ptr::addr_of!(elephc_probe_verify_fn).read() };
        if addr == 0 {
            return false;
        }
        let verify = unsafe { std::mem::transmute::<usize, VerifyFn>(addr) };
        (unsafe { verify(value.as_ptr(), value.len()) }) != 0
    }

    /// Starts or ends this request's own profile, when the binary carries hooks.
    ///
    /// A production binary can ship instrumented and dormant (`ELEPHC_INSTR_OFF=1`),
    /// costing a load and a branch per call, and still answer the exact same
    /// question a dev build answers — for the one request that asked. That is the
    /// difference between "profiling is a dev thing" and "profiling is a thing you
    /// can do where the problem actually is".
    pub fn profile_request(begin: bool) {
        let addr = unsafe { std::ptr::addr_of!(elephc_instr_request_fn).read() };
        if addr == 0 {
            return;
        }
        let bracket = unsafe { std::mem::transmute::<usize, RequestFn>(addr) };
        unsafe { bracket(u32::from(begin)) };
    }

    /// Opens the exact profiler's trace context for this request from the
    /// inbound W3C `traceparent` (absent → a new trace is started). Pass the
    /// header value, or `None`. No-op unless `--instrument` filled the slot.
    /// `route` is passed alongside so an exact capture can be broken down per
    /// endpoint, the way the sampler's route tagging already allows.
    pub fn trace_begin(traceparent: Option<&str>, route: &str) {
        let addr = unsafe { std::ptr::addr_of!(elephc_instr_trace_fn).read() };
        if addr == 0 {
            return;
        }
        let begin = unsafe { std::mem::transmute::<usize, TraceFn>(addr) };
        let (tp, tp_len) = match traceparent {
            Some(value) => (value.as_ptr(), value.len()),
            None => (std::ptr::null(), 0),
        };
        unsafe { begin(tp, tp_len, route.as_ptr(), route.len()) };
    }
}

fn run_handler(handler: extern "C" fn()) -> Vec<u8> {
    request_state::set_capture(true);
    request_state::clear_body();
    request_state::reset_response();
    // Arm the execution-timeout watchdog around the blocking handler, if enabled.
    let secs = MAX_EXEC_SECS.load(Ordering::Relaxed);
    if secs > 0 {
        unsafe { libc::alarm(secs); }
    }
    handler();
    if secs > 0 {
        unsafe { libc::alarm(0); }
    }
    request_state::take_body()
}
