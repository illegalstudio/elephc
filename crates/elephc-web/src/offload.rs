//! Purpose:
//! Handler offload (`--handler-offload`): run the blocking PHP handler on ONE
//! dedicated `php-handler` OS thread per worker, fed a bounded mpsc job queue by
//! the tokio I/O thread, so request/response I/O of other connections overlaps
//! PHP execution. Owns the cross-thread job/response types, the handler-thread
//! spawn helper, the SIGALRM mask plumbing, the classic per-job body
//! (`run_one_job`), and the I/O-thread-built overload responses (503/500).
//!
//! Called from:
//! - `crate::worker::serve` (classic `--web`) and
//!   `crate::worker_mode::enter_worker_loop` (`--web-worker[=script]`), which
//!   spawn the handler thread and, in their `service_fn`, hand each parsed
//!   request across the channel when offload is enabled.
//!
//! Key details:
//! - Handler-thread affinity is the correctness core: the `php-handler` thread is
//!   the ONLY thread that ever touches any PHP-visible state (`set_request`, the
//!   `take_*` drains, the capture flag, `RESPONSE_*`, `TMP_FILES`, GC, and
//!   `handler()` itself). The I/O thread only moves owned `Send` values
//!   (`RequestJob`/`ResponseParts`) through the channels; the mpsc `send`/`recv`
//!   and oneshot `send`/`await` edges provide the happens-before for the
//!   `CString`/`Bytes` payloads, so no runtime state is made thread-safe and no
//!   `unsafe impl Send` is needed.
//! - The runtime stays a current-thread tokio runtime; the handler thread is a
//!   plain `std::thread` consuming with `blocking_recv()` (never a tokio worker),
//!   with an explicit 8 MiB stack so PHP recursion depth does not shrink.
//! - exit/die works by construction: the compiled `_elephc_web_handler` prologue
//!   holds the `setjmp` bailout anchor, so moving the `handler()` call to this
//!   thread moves the anchor with it and the `longjmp` stays same-thread/
//!   same-stack. No compiler/codegen change.
//! - `--max-execution-time`: SIGALRM is blocked on the I/O thread and unblocked
//!   on the handler thread, so a runaway-handler alarm is delivered to the
//!   handler thread deterministically. A Rust panic escaping the job loop is
//!   caught once at the outermost level and turned into `_exit(1)` so the master
//!   respawns the worker (never a half-alive worker, never per-job recovery).

use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Response;
use tokio::sync::{mpsc, oneshot};

use crate::request_state::{self, RequestMeta};

/// A parsed request handed from the I/O thread to the handler thread. Every field
/// is plain owned data that is already `Send` (`String`, `CString`, `Bytes`,
/// `Vec`, `RequestMeta`, and a `oneshot::Sender<ResponseParts>`), so the job moves
/// across the thread boundary with no `unsafe impl Send`. The channel `send`/`recv`
/// edge publishes the header `CString`s and body `Bytes` to the handler thread.
pub(crate) struct RequestJob {
    /// HTTP method (e.g. `"GET"`), moved verbatim into `set_request`.
    pub method: String,
    /// Full request URI (`$_SERVER['REQUEST_URI']`).
    pub uri: String,
    /// Request path component.
    pub path: String,
    /// Query string (without the leading `?`).
    pub query: String,
    /// Request headers already in their final `(name, value, php_name)` `CString`
    /// form (built on the I/O thread by `request_header_cstrings`).
    pub headers: Vec<(CString, CString, CString)>,
    /// Fully-collected request body (hyper's `Bytes`, refcount-shared, atomic).
    pub body: Bytes,
    /// Connection/server metadata backing the remaining `$_SERVER` keys.
    pub meta: RequestMeta,
    /// One-shot channel back to the connection task with the produced response.
    pub reply: oneshot::Sender<ResponseParts>,
}

/// The owned response the handler thread produces and the I/O thread consumes.
/// Drained from the response statics on the handler thread, then moved back to the
/// connection task through the oneshot; after the oneshot resolves the I/O thread
/// owns these bytes outright (gzip runs there, on this owned body).
pub(crate) struct ResponseParts {
    /// HTTP status code drained from `RESPONSE_STATUS`.
    pub status: u16,
    /// Response headers drained from `RESPONSE_HEADERS`, in send order.
    pub headers: Vec<(String, String)>,
    /// Captured response body drained from `RESPONSE_BODY`.
    pub body: Vec<u8>,
}

/// RAII guard that disarms any pending `--max-execution-time` alarm when dropped.
/// Installed around each job in the `php-handler` thread's loop so that a Rust
/// panic escaping the job (which unwinds past the plain `alarm(0)` disarm
/// statement in `run_one_job` / `run_worker_handler`) still cancels the alarm
/// during unwinding, BEFORE the outer `catch_unwind` returns and BEFORE the
/// pending SIGALRM can fire and kill the worker via its default action. On the
/// normal (non-panic) path the `Drop` is a harmless no-op disarm (the job already
/// disarmed). Covers BOTH the classic `run_one_job` and the worker-mode
/// `run_one_worker_job` paths at the single chokepoint: every job goes through the
/// `run_job` closure in `handler_thread_main`.
struct AlarmDisarmGuard;

impl Drop for AlarmDisarmGuard {
    fn drop(&mut self) {
        // SAFETY: alarm(0) is async-signal-safe and process-wide; disarming a
        // pending alarm is always safe regardless of the caller's state.
        unsafe {
            libc::alarm(0);
        }
    }
}

/// Blocks or unblocks `SIGALRM` on the CURRENT thread via `pthread_sigmask`. Used
/// so the `--max-execution-time` alarm (armed on the handler thread around
/// `handler()`) is delivered to the handler thread deterministically: the I/O
/// thread blocks it before spawning the handler thread (the child inherits the
/// block), and the handler thread unblocks it at its own start.
fn set_sigalrm_blocked(block: bool) {
    // SAFETY: pthread_sigmask on a zero-initialized sigset containing only
    // SIGALRM; it only changes this thread's signal mask.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGALRM);
        let how = if block { libc::SIG_BLOCK } else { libc::SIG_UNBLOCK };
        libc::pthread_sigmask(how, &set, std::ptr::null_mut());
    }
}

/// Blocks `SIGALRM` on the I/O thread. Called on the main/I/O thread BEFORE the
/// handler thread is spawned (when `--handler-offload` is on) so the spawned
/// child inherits the block and a runaway-handler alarm cannot be delivered to
/// the I/O thread.
pub(crate) fn block_sigalrm_on_io_thread() {
    set_sigalrm_blocked(true);
}

/// Stack size for the `php-handler` thread (8 MiB), matching the main thread's
/// stack so PHP recursion depth does not silently shrink (Rust's spawned-thread
/// default is only 2 MiB).
const HANDLER_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Spawns the single long-lived `php-handler` thread that drains the job queue and
/// runs the per-request PHP body via `run_job`. The thread has an explicit 8 MiB
/// stack, unblocks `SIGALRM` at its start (so the execution-timeout alarm lands
/// here), and consumes jobs with `blocking_recv()` (it is a plain `std::thread`,
/// not a tokio worker, so the runtime stays current-thread). A Rust panic escaping
/// the loop is caught once at the outermost level and converted to `_exit(1)` so
/// the master respawns the worker; the loop is never resumed after a panic because
/// PHP state is indeterminate mid-request. When every `Sender` is dropped (worker
/// shutdown) `blocking_recv()` returns `None`, the loop ends, and the thread exits
/// cleanly. The `JoinHandle` is detached: the process exits through the main
/// thread's normal teardown.
pub(crate) fn spawn_handler_thread<F>(rx: mpsc::Receiver<RequestJob>, run_job: F)
where
    F: FnMut(RequestJob) + Send + 'static,
{
    std::thread::Builder::new()
        .name("php-handler".to_string())
        .stack_size(HANDLER_STACK_SIZE)
        .spawn(move || handler_thread_main(rx, run_job))
        .expect("failed to spawn php-handler thread");
}

/// Body of the `php-handler` thread: unblock `SIGALRM`, then drain the queue under
/// one outermost `catch_unwind`. On a caught panic it writes an async-signal-safe
/// diagnostic and `_exit(1)`s so supervision respawns the worker. See
/// `spawn_handler_thread`.
fn handler_thread_main<F>(mut rx: mpsc::Receiver<RequestJob>, mut run_job: F)
where
    F: FnMut(RequestJob),
{
    // Undo the I/O thread's SIGALRM block (inherited across spawn) so the
    // execution-timeout alarm armed around handler() is delivered here.
    set_sigalrm_blocked(false);
    let looped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        while let Some(job) = rx.blocking_recv() {
            // RAII disarm: on a Rust panic escaping `run_job`, `Drop` runs during
            // unwinding and cancels any armed `--max-execution-time` alarm BEFORE
            // the outer `catch_unwind` returns and before SIGALRM can fire.
            let _alarm_guard = AlarmDisarmGuard;
            run_job(job);
        }
    }));
    if looped.is_err() {
        // Cancel any alarm that survived the unwind (defense in depth; the per-job
        // AlarmDisarmGuard already disarms during unwinding, but keep this for
        // clarity and the narrow post-`catch_unwind` window).
        unsafe {
            libc::alarm(0);
        }
        // A panic unwound out of the job loop (never PHP itself, which routes
        // fatals through __rt_exit): the worker is in an indeterminate state, so
        // die and let the master respawn rather than accept-and-never-answer.
        const MSG: &[u8] = b"elephc-web: php-handler thread panicked; recycling worker\n";
        // SAFETY: write(2) + _exit(2) are async-signal-safe and touch no Rust state.
        unsafe {
            libc::write(2, MSG.as_ptr() as *const libc::c_void, MSG.len());
            libc::_exit(1);
        }
    }
}

/// Runs the CLASSIC (`--web`) per-request PHP body for one job on the handler
/// thread and replies with the produced `ResponseParts`. This is exactly the
/// inline `set_request` + `run_handler` sequence, moved verbatim: install the
/// request into the handler-thread-affine statics, enable capture, reset the
/// response, arm the `--max-execution-time` watchdog around the blocking
/// `handler()`, disarm it, count the served request, then drain status/headers/
/// body. A dropped receiver (client gone) makes `reply.send` a no-op.
///
/// Factored as a free function so the future ZTS (N>1) work can wrap it in a lock
/// without touching the call sites. `served` is the caller's process-local
/// `SERVED` counter (`worker::SERVED`), incremented here so the count reflects
/// handled requests on the handler thread.
pub(crate) fn run_one_job(
    job: RequestJob,
    handler: extern "C" fn(),
    max_exec_secs: u32,
    served: &AtomicUsize,
) {
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
    request_state::set_capture(true);
    request_state::clear_body();
    request_state::reset_response();
    // Arm the execution-timeout watchdog around the blocking handler, if enabled.
    // The alarm fires on THIS thread (SIGALRM is unblocked here, blocked on the
    // I/O thread), so a runaway handler recycles the worker deterministically.
    if max_exec_secs > 0 {
        // SAFETY: alarm(2) is process-wide and async-signal-safe.
        unsafe {
            libc::alarm(max_exec_secs);
        }
    }
    handler();
    if max_exec_secs > 0 {
        // SAFETY: disarm the alarm now that the handler returned.
        unsafe {
            libc::alarm(0);
        }
    }
    served.fetch_add(1, Ordering::Relaxed);
    let body = request_state::take_body();
    let status = request_state::take_status();
    let headers = request_state::take_headers();
    let _ = reply.send(ResponseParts {
        status,
        headers,
        body,
    });
}

/// Builds the queue-full `503 Service Unavailable` response ENTIRELY on the I/O
/// thread (no PHP): the handler thread is busy and the bounded queue is full, so
/// the request is shed immediately with `Retry-After: 1` rather than growing an
/// unbounded backlog. Load-balancer friendly and observable, unlike accept-pausing.
pub(crate) fn queue_full_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(503)
        .header("retry-after", "1")
        .body(Full::new(Bytes::from_static(b"Service Unavailable")))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))))
}

/// Builds the `500 Internal Server Error` response used when the handler thread
/// dropped the reply oneshot (its panic/exit raced this connection task). Built on
/// the I/O thread; relevant only in the narrow window before `_exit(1)` wins.
pub(crate) fn handler_gone_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(500)
        .body(Full::new(Bytes::from_static(b"Internal Server Error")))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Purpose:
    // Unit tests for the offload channel machinery without a compiled program: a
    // stub `extern "C"` handler drives `run_one_job` end-to-end over a real
    // oneshot and the response statics.
    //
    // Called from:
    // - `cargo test` through Rust's test harness.
    //
    // Key details:
    // - `run_one_job` mutates the shared per-request process statics (REQ_*,
    //   RESPONSE_*, capture), so the test holds `REQUEST_STATE_TEST_LOCK` to
    //   serialize against `request_state`'s own tests. No tokio runtime is needed:
    //   the handler runs synchronously and the oneshot value is read with
    //   `try_recv` after `run_one_job` returns.

    /// Stub handler standing in for the compiled `_elephc_web_handler`: writes a
    /// body through the capture sink and sets a non-default status, so the round
    /// trip has observable, non-empty `ResponseParts`.
    extern "C" fn stub_handler() {
        // SAFETY: writes 2 valid bytes to the response body and sets the status
        // through the bridge's own C-ABI setters (single-threaded test).
        unsafe {
            request_state::elephc_web_write(b"hi".as_ptr(), 2);
            request_state::elephc_web_set_status(201);
        }
    }

    /// Verifies `run_one_job` runs the stub handler and replies with the captured
    /// body/status: a job sent with a fresh oneshot yields `ResponseParts { status
    /// 201, body "hi" }`, and the served counter is incremented once.
    #[test]
    fn run_one_job_round_trips_response_parts() {
        let _guard = crate::request_state::REQUEST_STATE_TEST_LOCK.lock().unwrap();
        let served = AtomicUsize::new(0);
        let (tx, mut rx) = oneshot::channel();
        let job = RequestJob {
            method: "GET".into(),
            uri: "/x".into(),
            path: "/x".into(),
            query: "".into(),
            headers: Vec::new(),
            body: Bytes::from_static(b""),
            meta: RequestMeta {
                remote_addr: "127.0.0.1".into(),
                remote_port: 5000,
                server_addr: "127.0.0.1".into(),
                server_port: 8080,
                protocol: "HTTP/1.1",
                https: false,
            },
            reply: tx,
        };
        run_one_job(job, stub_handler, 0, &served);
        let parts = rx.try_recv().expect("handler must have replied");
        assert_eq!(parts.status, 201, "status must come from the handler");
        assert_eq!(parts.body, b"hi", "body must be the captured output");
        assert!(parts.headers.is_empty(), "stub set no headers");
        assert_eq!(served.load(Ordering::Relaxed), 1, "served must increment once");
    }

    /// Verifies the overload responses are built without touching PHP state: the
    /// 503 carries `Retry-After` and the 500 is a plain internal-error body.
    #[test]
    fn overload_responses_have_expected_status() {
        let full = queue_full_response();
        assert_eq!(full.status(), 503);
        assert_eq!(
            full.headers().get("retry-after").map(|v| v.as_bytes()),
            Some(b"1".as_ref())
        );
        let gone = handler_gone_response();
        assert_eq!(gone.status(), 500);
    }
}
