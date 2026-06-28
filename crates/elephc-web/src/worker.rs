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

use crate::request_state;

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
        loop {
            // --max-requests recycling: stop accepting once the cap is reached so
            // the master respawns a fresh worker (bounds memory growth over time).
            if max_requests > 0 && SERVED.load(Ordering::Relaxed) >= max_requests {
                break;
            }
            let (stream, peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            tokio::task::spawn_local(http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(Duration::from_secs(30))
                .serve_connection(io, service_fn(move |req: Request<hyper::body::Incoming>| async move {
                    let started = Instant::now();
                    let method = req.method().as_str().to_string();
                    let uri = req.uri().to_string();
                    let path = req.uri().path().to_string();
                    let query = req.uri().query().unwrap_or("").to_string();
                    let protocol = format!("{:?}", req.version());
                    // Captured for the optional access log (method/path are moved into set_request).
                    let log_method_path = if access_log { Some((method.clone(), path.clone())) } else { None };
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
                    let collected = if max_body == 0 {
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
                    request_state::set_request(method, uri, path, query, headers, body, meta);
                    let resp_body = run_handler(handler);
                    let status = request_state::take_status();
                    let resp_headers = request_state::take_headers();
                    // gzip the body when the client accepts it, the body is large
                    // enough to be worth it, and the handler did not already set a
                    // Content-Encoding.
                    let already_encoded = resp_headers
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("content-encoding"));
                    let gzipped = if accepts_gzip && !already_encoded && resp_body.len() >= GZIP_MIN_LEN {
                        gzip_bytes(&resp_body)
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
                    Ok::<_, Infallible>(response)
                })));
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

/// Runs the PHP handler for one request and returns the captured response body.
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
    SERVED.fetch_add(1, Ordering::Relaxed);
    request_state::take_body()
}
