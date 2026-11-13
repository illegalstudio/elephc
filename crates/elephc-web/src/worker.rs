//! Purpose:
//! Per-worker HTTP serving: take the worker listening socket (shared on Apple,
//! per-worker `SO_REUSEPORT` on Linux, or the inherited Unix listener), run a
//! tokio current-thread runtime, and dispatch each request to the PHP handler.
//!
//! Called from:
//! - `crate::server::spawn_worker` in each forked child process.
//!
//! Key details:
//! - current-thread runtime + a blocking handler() call means PHP never runs on
//!   two threads in one worker; concurrency comes from the N forked workers.
//! - Linux uses one `SO_REUSEPORT` socket per worker; Apple targets accept on
//!   inherited duplicates of the master socket; Unix sockets use the inherited
//!   listener on every target.

use std::convert::Infallible;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::sync::watch;

use crate::forwarded::{self, TrustedProxies};
use crate::listener::{ListenSource, Listener};
use crate::probe_route;
use crate::request_state;
use crate::session::upload_progress;

/// Number of requests this worker has served, used by `--max-requests` recycling.
/// Process-local (each forked worker has its own copy starting at 0).
static SERVED: AtomicUsize = AtomicUsize::new(0);

/// Application workers stop accepting and drain connections when a termination signal arrives.
static APPLICATION_STOP: AtomicBool = AtomicBool::new(false);

/// Records a stop request using only async-signal-safe operations.
extern "C" fn application_shutdown_signal(_: libc::c_int) {
    APPLICATION_STOP.store(true, Ordering::Relaxed);
}

/// Installs cooperative worker shutdown before running the post-fork PHP bootstrap.
pub(crate) fn install_application_shutdown() {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = application_shutdown_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        elephc_process_context::__elephc_process_register_worker_handler(action.sa_sigaction);
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut());
    }
}

/// Defers termination signals until PHP returns, including reads with socket receive deadlines.
pub(crate) fn with_application_signals_blocked<T>(callback: impl FnOnce() -> T) -> T {
    let guard = elephc_process_context::Deferral::new();
    let result = callback();
    drop(guard);
    result
}

/// Records one completed handler and broadcasts a graceful recycle at the quota.
fn record_completed_request(max_requests: usize, recycle: &watch::Sender<bool>) {
    // Unbounded recycling is the default; do not pay an atomic read-modify-write
    // per request when the counter cannot trigger anything.
    if max_requests == 0 {
        return;
    }
    let served = SERVED.fetch_add(1, Ordering::Relaxed) + 1;
    if served >= max_requests {
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

/// Per-worker serving configuration. It is cloned into each connection task
/// because the trusted-proxy allowlist owns a shared allocation.
#[derive(Clone)]
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
    /// Deadline for RECEIVING the whole request body, in seconds; `0` = no limit.
    ///
    /// The header-read timeout below bounds only the head of the request. Without this a
    /// client could finish its headers and then send a declared body indefinitely slowly:
    /// the size cap applies to bytes that have ARRIVED, so it bounds memory but not
    /// connection lifetime, and enough such connections exhaust a worker's descriptors and
    /// task slots (issue #887).
    pub body_read_secs: u64,
    /// Peers allowed to supply Forwarded / X-Forwarded-* headers.
    pub trusted_proxies: TrustedProxies,
}

/// Minimum response size (bytes) worth gzip-compressing; below this the framing
/// overhead outweighs the savings.
const GZIP_MIN_LEN: usize = 256;

/// Serves HTTP on `source` in this worker process. Builds a current-thread
/// tokio runtime and loops accepting connections, serving each with the PHP
/// handler per `WorkerConfig`.
pub fn serve(source: ListenSource, handler: extern "C" fn(), cfg: WorkerConfig) {
    serve_inner::<false>(source, handler, cfg);
}

/// Serves with cooperative signal draining for a bootstrapped application worker.
pub(crate) fn serve_application(source: ListenSource, handler: extern "C" fn(), cfg: WorkerConfig) {
    serve_inner::<true>(source, handler, cfg);
}

/// Polls application shutdown without constructing timers in the legacy specialization.
async fn application_tick<const APPLICATION: bool>() {
    if APPLICATION { tokio::time::sleep(Duration::from_millis(50)).await; }
    else { std::future::pending::<()>().await; }
}

/// Shares HTTP transport while compiling application-only signal checks out of the default path.
fn serve_inner<const APPLICATION: bool>(source: ListenSource, handler: extern "C" fn(), cfg: WorkerConfig) {
    let WorkerConfig {
        max_body,
        max_requests,
        access_log,
        max_exec_secs,
        gzip,
        body_read_secs,
        trusted_proxies,
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
    let (server_addr, server_port) = match &source {
        ListenSource::Tcp(addr) => (addr.ip().to_string(), addr.port()),
        #[cfg(target_vendor = "apple")]
        ListenSource::TcpPassed(_, addr) => (addr.ip().to_string(), addr.port()),
        ListenSource::Unix(_) => (String::new(), 0),
    };
    request_state::set_server_addr(&server_addr, server_port);
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
        let mut listener = match Listener::bind(source) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("elephc-web: failed to bind listener: {error}");
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
            if APPLICATION && APPLICATION_STOP.load(Ordering::Relaxed) {
                let _ = recycle.send(true);
                break;
            }
            if *recycle_requested.borrow() {
                break;
            }
            let accepted = tokio::select! {
                _ = application_tick::<APPLICATION>(), if APPLICATION => {
                    continue;
                }
                changed = recycle_requested.changed() => {
                    if changed.is_err() || *recycle_requested.borrow() {
                        break;
                    }
                    continue;
                }
                accepted = listener.accept() => accepted,
            };
            // The accept future and quota notification can become ready in the same poll.
            if APPLICATION && (APPLICATION_STOP.load(Ordering::Relaxed) || *recycle_requested.borrow()) {
                drop(accepted);
                break;
            }
            let (stream, peer) = match accepted {
                Ok(pair) => pair,
                // The master owns the descriptor channel. EOF means it is gone,
                // so this worker has no listener and must not spin.
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionAborted => break,
                Err(_) => continue,
            };
            // Small response writes must leave immediately. Without TCP_NODELAY the
            // Nagle/delayed-ACK interaction stalls a keep-alive response for tens of
            // milliseconds, which dominates the request under load.
            let _ = stream.set_nodelay(true);
            let io = TokioIo::new(stream);
            let request_recycle = recycle.clone();
            let trusted = trusted_proxies.clone();
            let mut connection_recycle = recycle.subscribe();
            connections.push(tokio::task::spawn_local(async move {
                let connection = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(30))
                    .serve_connection(io, service_fn(move |req: Request<hyper::body::Incoming>| {
                        let request_recycle = request_recycle.clone();
                        let trusted = trusted.clone();
                        async move {
                    // Seed session deployment config before upload-progress body
                    // draining. The PHP prelude repeats this reset immediately
                    // before the handler, so both phases see identical values and
                    // no request can inherit the prior request's session settings.
                    unsafe { crate::session::elephc_web_session_reset() };
                    // Only the access log needs the request clock. Reading it on
                    // every request costs a `mach_absolute_time`/vDSO call even
                    // when no log line will be written.
                    let started = access_log.then(Instant::now);
                    // Capture the request head once, as NUL-terminated strings the
                    // prelude getters can hand to PHP. `cstring` allocates once per
                    // value; the old `String` + `replace` + `CString` chain allocated
                    // three times.
                    let method = request_state::cstring(req.method().as_str().as_bytes());
                    let uri = request_state::cstring_display(req.uri());
                    let path = request_state::cstring(req.uri().path().as_bytes());
                    let query_text = req.uri().query().unwrap_or("").to_string();
                    let query = request_state::cstring(query_text.as_bytes());
                    let protocol = request_state::cstring(http_protocol(req.version()).as_bytes());
                    // `HeaderMap` lookups are hash-based and case-insensitive, and
                    // allocate only when the header is present. The old code
                    // materialized every header as two `String`s and then scanned the
                    // vector three more times.
                    let header = |name: &str| {
                        req.headers()
                            .get(name)
                            .and_then(|value| value.to_str().ok())
                            .map(str::to_owned)
                    };
                    let content_type = header("content-type");
                    let content_length = header("content-length");
                    // Upload progress only needs the Cookie header for a
                    // multipart/form-data body; materialize it only for that
                    // rare shape.
                    let req_cookie = if content_type.as_deref().is_some_and(|value| {
                        contains_ascii_case_insensitive(value.as_bytes(), b"multipart/form-data")
                    }) {
                        header("cookie")
                    } else {
                        None
                    };
                    let req_traceparent = if probe_route::trace_available() {
                        header("traceparent")
                    } else {
                        None
                    };
                    let profile_header = if probe_route::query_available() {
                        header("x-elephc-query")
                    } else {
                        None
                    };
                    // Signed, not merely present: the value is
                    // `t=<unix seconds>,v=<hmac>` over the build key, so a header
                    // captured from a log stops working within minutes and one
                    // invented from scratch never does.
                    let profile_this = profile_header
                        .as_deref()
                        .is_some_and(probe_route::query_is_authentic);
                    // Captured for the optional access log (method/path are moved into set_request).
                    let log_method_path = if access_log {
                        Some((
                            method.to_string_lossy().into_owned(),
                            path.to_string_lossy().into_owned(),
                        ))
                    } else {
                        None
                    };
                    // The route label the sampling probe stamps onto samples taken
                    // during this request (no-op unless the binary was built --probe).
                    // Shaped, not raw: the probe's route table is fixed at 256 with
                    // no eviction, and raw paths spend it on identifiers.
                    let probe_route = if probe_route::active() {
                        probe_route::route_label(
                            &method.to_string_lossy(),
                            &path.to_string_lossy(),
                        )
                    } else {
                        String::new()
                    };
                    let accepts_gzip = gzip
                        && req.headers().get(hyper::header::ACCEPT_ENCODING).is_some_and(|v| {
                            contains_ascii_case_insensitive(v.as_bytes(), b"gzip")
                        });
                    let headers: Vec<(CString, CString)> = req
                        .headers()
                        .iter()
                        .map(|(n, v)| {
                            (
                                request_state::cstring(n.as_str().as_bytes()),
                                request_state::cstring(v.as_bytes()),
                            )
                        })
                        .collect();
                    // Avoid materializing a second header list on the common
                    // path where no proxy is trusted.
                    let forwarded = if trusted.is_empty() {
                        None
                    } else {
                        let forwarded_headers: Vec<(String, String)> = req
                            .headers()
                            .iter()
                            .map(|(name, value)| {
                                (
                                    name.as_str().to_string(),
                                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                                )
                            })
                            .collect();
                        Some(forwarded::apply(&peer, &forwarded_headers, &trusted))
                    };
                    // The body must be fully collected (async) BEFORE the blocking handler
                    // runs, since handler() cannot yield on the current-thread runtime.
                    // Collect with a size cap (0 = unlimited); an over-limit body
                    // short-circuits to 413 without ever running the PHP handler.
                    //
                    // For a tracked `session.upload_progress` multipart upload, the body
                    // is drained frame-by-frame so progress can be written to the session
                    // file as bytes arrive, while still buffering the complete body the
                    // handler sees. Non-tracked requests keep the simple fast path.
                    //
                    // THE WHOLE COLLECTION IS UNDER ONE DEADLINE, every path included: a
                    // stalled plain body, a size-limited one, and a tracked multipart upload
                    // are all ways to hold a connection open forever otherwise (issue #887).
                    // Expiry answers 408 and does not keep the connection alive.
                    // A request without Content-Length or Transfer-Encoding has
                    // no body (RFC 9112 §6.3). Short-circuit before building the
                    // body future and its read deadline: this is every GET and
                    // HEAD served by a healthy keep-alive client.
                    let request_has_body = request_declares_body(req.headers());
                    let body = if request_has_body {
                        let collect_body = async {
                            if let Some(mut tracker) = upload_progress::begin(
                                content_type.as_deref(),
                                req_cookie.as_deref(),
                                content_length.as_deref(),
                                &query_text,
                            ) {
                                drain_with_progress(req.into_body(), max_body, &mut tracker).await
                            } else if max_body == 0 {
                                req.into_body()
                                    .collect()
                                    .await
                                    .map(|c| c.to_bytes().to_vec())
                                    .map_err(|_| ())
                            } else {
                                Limited::new(req.into_body(), max_body)
                                    .collect()
                                    .await
                                    .map(|c| c.to_bytes().to_vec())
                                    .map_err(|_| ())
                            }
                        };
                        let collected = if body_read_secs == 0 {
                            Ok(collect_body.await)
                        } else {
                            tokio::time::timeout(Duration::from_secs(body_read_secs), collect_body)
                                .await
                                .map_err(|_| ())
                        };
                        match collected {
                            Ok(Ok(b)) => b,
                            Err(()) => {
                                let resp = Response::builder()
                                    .status(408)
                                    .header(hyper::header::CONNECTION, "close")
                                    .body(Full::new(Bytes::from_static(b"Request Timeout")))
                                    .unwrap_or_else(|_| {
                                        Response::new(Full::new(Bytes::from_static(b"")))
                                    });
                                return Ok::<_, Infallible>(resp);
                            }
                            Ok(Err(_)) => {
                                let resp = Response::builder()
                                    .status(413)
                                    .body(Full::new(Bytes::from_static(b"Payload Too Large")))
                                    .unwrap_or_else(|_| {
                                        Response::new(Full::new(Bytes::from_static(b"")))
                                    });
                                return Ok::<_, Infallible>(resp);
                            }
                        }
                    } else {
                        Vec::new()
                    };
                    let remote_addr = match &forwarded {
                        Some(facts) => request_state::cstring(facts.remote_addr.as_bytes()),
                        None => peer
                            .remote_ip()
                            .map(request_state::ip_cstring)
                            .unwrap_or_default(),
                    };
                    let request_scheme = forwarded.as_ref().and_then(|facts| {
                        (facts.request_scheme != "http")
                            .then(|| request_state::cstring(facts.request_scheme.as_bytes()))
                    });
                    let meta = request_state::RequestMeta {
                        remote_addr,
                        remote_port: forwarded
                            .as_ref()
                            .map(|facts| facts.remote_port)
                            .unwrap_or_else(|| peer.remote_port()),
                        server_port: forwarded
                            .as_ref()
                            .map(|facts| facts.server_port)
                            .unwrap_or_else(|| peer.server_port()),
                        request_scheme,
                        https: forwarded.as_ref().is_some_and(|facts| facts.https),
                        protocol,
                    };
                    request_state::set_request(method, uri, path, query, headers, body, meta);
                    probe_route::set(&probe_route);
                    // A signed header authorizes this request outright; without
                    // one, offer the request anyway — the instrumentation starts
                    // a slice only if something is waiting for one, which is how
                    // `monitor <address> --exact` gets an answer without a second
                    // way into the request path. The offer costs a call through a
                    // slot and a flag read.
                    //
                    // Before the trace, not after: this call is what decides
                    // whether this request is being profiled at all, and the
                    // trace context is dormant until it says so. The other order
                    // meant an idle service paid for a trace id every request.
                    probe_route::profile_request_kind(if profile_this { 1 } else { 2 });
                    probe_route::trace_begin(req_traceparent.as_deref(), &probe_route);
                    let resp_body = if APPLICATION {
                        with_application_signals_blocked(|| run_handler(handler))
                    } else {
                        run_handler(handler)
                    };
                    // Unconditional and idempotent: a request that started no
                    // slice ends none, and consecutive profiled requests stay
                    // separate captures.
                    probe_route::profile_request_kind(0);
                    probe_route::clear();
                    let status = request_state::take_status();
                    let mut resp_headers = request_state::take_headers();
                    // Propagate the session id through same-origin URLs / forms when
                    // `session.use_trans_sid` is active (no-op otherwise). Runs on the
                    // plaintext body, before the gzip decision below.
                    let resp_body = crate::trans_sid::maybe_rewrite_response(
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
                            peer.log_peer(),
                            m,
                            p,
                            status,
                            started.map(|start| start.elapsed().as_millis()).unwrap_or_default()
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

/// Returns whether an HTTP/1 request declares a body.
///
/// RFC 9112 §6.3: without `Content-Length` or `Transfer-Encoding`, the request
/// has no body. This is the fast path for every GET and HEAD served by a healthy
/// keep-alive client; it lets the worker skip constructing the body future and
/// its read deadline entirely. A `Content-Length: 0` also declares an empty body.
pub(crate) fn request_declares_body(headers: &hyper::HeaderMap) -> bool {
    headers
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() != "0")
        || headers.contains_key(hyper::header::TRANSFER_ENCODING)
}

/// Returns whether `haystack` contains `needle`, ignoring ASCII case.
///
/// Used for the `Accept-Encoding` token check so a keep-alive request does not
/// allocate a lowercased copy of the header just to look for `gzip`.
fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Returns the PHP-facing protocol string for a Hyper version.
fn http_protocol(version: hyper::Version) -> &'static str {
    match version {
        hyper::Version::HTTP_09 => "HTTP/0.9",
        hyper::Version::HTTP_10 => "HTTP/1.0",
        hyper::Version::HTTP_2 => "HTTP/2.0",
        hyper::Version::HTTP_3 => "HTTP/3.0",
        _ => "HTTP/1.1",
    }
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

/// Runs one request's PHP handler and returns the response bytes it produced.
///
/// Resets the per-request capture state first, arms the execution-timeout
/// watchdog around the blocking call when one is configured, and collects
/// whatever the handler wrote.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Accept-Encoding` probe is case-insensitive and allocation-free.
    #[test]
    fn gzip_token_check_ignores_case() {
        assert!(contains_ascii_case_insensitive(b"gzip, deflate", b"gzip"));
        assert!(contains_ascii_case_insensitive(b"br, GZip", b"gzip"));
        assert!(!contains_ascii_case_insensitive(b"br, deflate", b"gzip"));
        assert!(!contains_ascii_case_insensitive(b"", b"gzip"));
    }

    /// Body presence follows HTTP/1 framing, not the method name.
    #[test]
    fn body_presence_follows_framing_headers() {
        let mut headers = hyper::HeaderMap::new();
        assert!(!request_declares_body(&headers));
        headers.insert(
            hyper::header::CONTENT_LENGTH,
            hyper::header::HeaderValue::from_static("0"),
        );
        assert!(!request_declares_body(&headers));
        headers.insert(
            hyper::header::CONTENT_LENGTH,
            hyper::header::HeaderValue::from_static("5"),
        );
        assert!(request_declares_body(&headers));
        let mut chunked = hyper::HeaderMap::new();
        chunked.insert(
            hyper::header::TRANSFER_ENCODING,
            hyper::header::HeaderValue::from_static("chunked"),
        );
        assert!(request_declares_body(&chunked));
    }

    /// The PHP-facing protocol string covers every Hyper version the server can see.
    #[test]
    fn protocol_names_match_http_versions() {
        assert_eq!(http_protocol(hyper::Version::HTTP_10), "HTTP/1.0");
        assert_eq!(http_protocol(hyper::Version::HTTP_11), "HTTP/1.1");
        assert_eq!(http_protocol(hyper::Version::HTTP_2), "HTTP/2.0");
    }
}
