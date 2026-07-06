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

use std::cell::Cell;
use std::convert::Infallible;
use std::ffi::CString;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use socket2::{Domain, Protocol, SockRef, Socket, Type};
use tokio::io::unix::AsyncFd;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::request_state;

/// Pending-connection backlog for each worker's listening socket.
const LISTEN_BACKLOG: i32 = 1024;

/// Maps a hyper HTTP version to its `$_SERVER['SERVER_PROTOCOL']` string as a
/// `&'static str`, so the per-request path needs no allocation (and no `Debug`
/// formatting) to record the protocol. Shared with `crate::worker_mode`.
pub(crate) fn version_str(version: hyper::Version) -> &'static str {
    match version {
        v if v == hyper::Version::HTTP_09 => "HTTP/0.9",
        v if v == hyper::Version::HTTP_10 => "HTTP/1.0",
        v if v == hyper::Version::HTTP_11 => "HTTP/1.1",
        v if v == hyper::Version::HTTP_2 => "HTTP/2.0",
        v if v == hyper::Version::HTTP_3 => "HTTP/3.0",
        _ => "HTTP/1.1",
    }
}

/// Builds a listening std::net::TcpListener with SO_REUSEPORT set, bound to `addr`.
/// Shared with `crate::worker_mode` (worker mode reuses the same SO_REUSEPORT setup).
pub(crate) fn reuseport_listener(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    sock.listen(LISTEN_BACKLOG)?;
    Ok(sock.into())
}

/// Builds the master's SINGLE listening socket for `--dispatch master`: identical
/// to `reuseport_listener` but WITHOUT `set_reuse_port`, since only the master
/// binds the port (the workers never listen; they receive fds). Nonblocking so
/// `dispatch::master_loop` can `poll` + `accept4`/`accept` it. Called once by
/// `server::run_master` after all workers are forked.
pub(crate) fn plain_listener(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_reuse_address(true)?;
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
    /// Run `__rt_gc_collect_cycles` every N requests (worker mode only);
    /// `0` = never, `1` = every request. Defaults to `1` in worker mode.
    pub worker_gc_interval: u32,
    /// Close a keep-alive connection after this many responses so the client
    /// reconnects and SO_REUSEPORT re-picks a worker (sends `Connection: close`);
    /// `0` = unlimited, default `0` (opt-in; off preserves the original behavior).
    pub max_conn_requests: usize,
    /// Close a keep-alive connection that stays idle (no new request) for more
    /// than this many seconds so the client reconnects; `0` = never, default `0`
    /// (opt-in; off preserves the original behavior).
    pub idle_timeout_secs: u32,
}

/// Minimum response size (bytes) worth gzip-compressing; below this the framing
/// overhead outweighs the savings.
const GZIP_MIN_LEN: usize = 256;

/// The source of accepted connections for a worker's serve loop. `Kernel` owns a
/// SO_REUSEPORT listener and `accept()`s (the default, behaviorally-identical
/// path); `Master` receives already-accepted fds over the master socketpair
/// (`--dispatch master`, slot = 1). Abstracting the loop head lets the SAME
/// per-connection lifecycle (`drive_connection`, incl. PR2 TLS + PR1 keep-alive/
/// idle gating) run in both modes. Shared by `serve` and
/// `worker_mode::enter_worker_loop`.
pub(crate) enum ConnSource {
    /// Kernel dispatch: the worker owns a SO_REUSEPORT listener and accepts.
    Kernel(TcpListener),
    /// Master dispatch: the worker sends READY and receives an fd per connection.
    Master {
        /// The worker's socketpair end, registered for readiness with tokio.
        chan: AsyncFd<OwnedFd>,
    },
}

/// One step of a serve loop's connection source: a ready connection, a transient
/// error to skip, or a closed source (master gone → the worker exits cleanly).
pub(crate) enum NextConn {
    /// A ready connection: the stream, the remote peer, and the local server addr.
    Serve(TcpStream, SocketAddr, SocketAddr),
    /// A transient error; the caller should `continue` the loop.
    Retry,
    /// The source is closed (EOF on the socketpair): the worker exits.
    Closed,
}

impl ConnSource {
    /// Builds the master-dispatch source from the child socketpair-end fd: makes it
    /// nonblocking (required by `AsyncFd`) and wraps it as an `AsyncFd<OwnedFd>`.
    /// Called by the serve loop when a dispatch chan was installed pre-boot.
    pub(crate) fn master(chan_fd: RawFd) -> std::io::Result<ConnSource> {
        crate::dispatch::set_nonblocking(chan_fd)?;
        // SAFETY: chan_fd is this worker's own socketpair end, owned by the process.
        let owned = unsafe { OwnedFd::from_raw_fd(chan_fd) };
        Ok(ConnSource::Master {
            chan: AsyncFd::new(owned)?,
        })
    }

    /// Yields the next connection to serve. Kernel mode `accept()`s (peer from the
    /// accept, addr = the parsed listen addr). Master mode sends READY then awaits
    /// a received fd (see `next_master`). `kernel_addr` is used only by the kernel
    /// arm; the master arm derives the server addr via getsockname.
    pub(crate) async fn next(&self, kernel_addr: SocketAddr) -> NextConn {
        match self {
            ConnSource::Kernel(listener) => match listener.accept().await {
                Ok((stream, peer)) => NextConn::Serve(stream, peer, kernel_addr),
                Err(_) => NextConn::Retry,
            },
            ConnSource::Master { chan } => next_master(chan, kernel_addr).await,
        }
    }

    /// Whether connections are served serially (master, slot = 1) rather than
    /// concurrently via `spawn_local` (kernel). Selects the drive strategy so a
    /// master worker serves exactly one connection at a time.
    pub(crate) fn is_serial(&self) -> bool {
        matches!(self, ConnSource::Master { .. })
    }
}

/// Master-mode connection step: send the READY byte (the caller has already done
/// the cap-before-READY `--max-requests` check), await a received fd over the
/// socketpair, reconstruct a tokio `TcpStream` from it, and derive REMOTE_ADDR/
/// REMOTE_PORT via getpeername (there is no `accept()` in a master-mode worker).
/// The received fd is the RAW TCP socket BEFORE any TLS handshake — the worker
/// runs the handshake later in `drive_connection`, so no key material crosses the
/// socketpair. Returns `Closed` on EOF (master gone) so the worker exits `0`.
///
/// `listen_addr` is the parsed `--listen` address. The returned server address
/// uses `listen_addr`'s IP so `$_SERVER['SERVER_ADDR']` matches kernel mode
/// EXACTLY (a `0.0.0.0` wildcard bind must report `0.0.0.0`, not the concrete
/// getsockname IP); the server PORT still comes from getsockname (the bound port,
/// identical to `listen_addr`'s for a concrete bind).
async fn next_master(chan: &AsyncFd<OwnedFd>, listen_addr: SocketAddr) -> NextConn {
    // Send READY, gated on writability for the nonblocking socketpair end.
    loop {
        let mut guard = match chan.writable().await {
            Ok(g) => g,
            Err(_) => return NextConn::Closed,
        };
        match guard.try_io(|inner| crate::dispatch::send_ready(inner.get_ref().as_raw_fd())) {
            Ok(Ok(())) => break,
            Ok(Err(_)) => return NextConn::Closed, // master gone
            Err(_would_block) => continue,
        }
    }
    // Await a received fd from the master. READY was sent exactly once (above); the
    // recv is retried WITHOUT re-sending READY so the master never over-credits
    // this worker with a second connection.
    let fd = loop {
        let mut guard = match chan.readable().await {
            Ok(g) => g,
            Err(_) => return NextConn::Closed,
        };
        match guard.try_io(|inner| crate::dispatch::recv_fd(inner.get_ref().as_raw_fd())) {
            Ok(Ok(Some(fd))) => break fd,
            Ok(Ok(None)) => return NextConn::Closed, // EOF: master gone
            // A hard recvmsg error did NOT dequeue the pending SCM_RIGHTS message,
            // so the fd is still buffered: retry the recv (EINTR) without re-sending
            // READY. Any other hard error is treated as fatal — exit and let the
            // master respawn — rather than re-sending READY (which would make the
            // master hand this worker a second connection → head-of-line blocking).
            Ok(Err(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Ok(Err(_)) => return NextConn::Closed,
            Err(_would_block) => continue,
        }
    };
    // Make the received fd nonblocking (+ SO_NOSIGPIPE on macOS) for tokio.
    if crate::dispatch::prepare_received_fd(fd).is_err() {
        // SAFETY: fd is a received descriptor owned here; drop it on setup failure.
        // The fd WAS consumed by recv_fd, so a subsequent READY re-send is correct.
        unsafe { libc::close(fd); }
        return NextConn::Retry;
    }
    // SAFETY: fd is a valid, connected, nonblocking TCP socket owned by this worker.
    let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
    // REMOTE_ADDR/REMOTE_PORT come from the socket's peer (getpeername), matching
    // what `accept()` would have reported in kernel mode.
    let peer = match SockRef::from(&std_stream).peer_addr().ok().and_then(|a| a.as_socket()) {
        Some(p) => p,
        None => return NextConn::Retry,
    };
    // SERVER address: keep the bound port from getsockname but take the IP from the
    // parsed listen address, so SERVER_ADDR is IDENTICAL to kernel mode (which
    // passes the parsed listen address) even on a wildcard bind.
    let server_port = match SockRef::from(&std_stream).local_addr().ok().and_then(|a| a.as_socket()) {
        Some(a) => a.port(),
        None => return NextConn::Retry,
    };
    let addr = SocketAddr::new(listen_addr.ip(), server_port);
    match TcpStream::from_std(std_stream) {
        Ok(stream) => NextConn::Serve(stream, peer, addr),
        Err(_) => NextConn::Retry,
    }
}

/// Drives ONE accepted connection's full lifecycle: run the TLS handshake (or the
/// plaintext passthrough) on the already-accepted `stream` via
/// `crate::tls::wrap_accepted` (PR2), build the hyper HTTP/1 connection, then
/// drive it with the PR1 idle-watchdog choice (`serve_connection_with_idle` when
/// `--idle-timeout` is on, else the plain future). Factored out of both serve
/// loops so the kernel path (`spawn_local(drive_connection(..))`, concurrent) and
/// the master path (`drive_connection(..).await`, slot = 1) share IDENTICAL
/// per-connection semantics. A failed handshake just drops the connection (the
/// worker survives), with an optional `--access-log` line.
pub(crate) async fn drive_connection<S>(
    stream: TcpStream,
    peer: SocketAddr,
    acceptor: Option<&'static TlsAcceptor>,
    http: http1::Builder,
    service: S,
    watchdog_activity: Option<Rc<Cell<Instant>>>,
    idle: Duration,
    access_log: bool,
) where
    S: hyper::service::HttpService<hyper::body::Incoming> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::ResBody: hyper::body::Body + 'static,
    <S::ResBody as hyper::body::Body>::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let io = match crate::tls::wrap_accepted(stream, acceptor).await {
        Some(io) => io,
        None => {
            if access_log {
                eprintln!("{} tls handshake failed", peer.ip());
            }
            return;
        }
    };
    let conn = http.serve_connection(TokioIo::new(io), service);
    match watchdog_activity {
        Some(wa) => serve_connection_with_idle(conn, wa, idle).await,
        None => {
            let _ = conn.await;
        }
    }
}

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
        worker_gc_interval: _,
        // Read straight off `cfg` (still valid: `WorkerConfig` is `Copy`) in the
        // close predicate; only the idle timeout needs a loop-invariant local.
        max_conn_requests: _,
        idle_timeout_secs,
    } = cfg;
    if max_exec_secs > 0 {
        MAX_EXEC_SECS.store(max_exec_secs, Ordering::Relaxed);
        install_exec_timeout_handler();
    }
    let listen_addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("elephc-web: invalid --listen address {:?}", listen);
            std::process::exit(1);
        }
    };
    // Master dispatch (`--dispatch master`) installs the child socketpair end into
    // a process-static slot before serve; kernel dispatch leaves it unset. Take it
    // now so the accept loop uses `ConnSource::Master` (receive fds, do NOT bind) or
    // `ConnSource::Kernel` (bind a SO_REUSEPORT listener, exactly as before).
    let child_chan = crate::dispatch::take_child_dispatch_chan();
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
        //
        // WI-4 / Q4 note on `header_read_timeout` idle semantics (hyper 1.10.1,
        // locked in Cargo.lock): hyper arms this timer inside `poll_read_head` as
        // soon as the connection starts waiting for a request head — which INCLUDES
        // the idle wait for the next keep-alive request. So on this version it
        // already doubles as an implicit ~30s idle bound, conflated with the
        // anti-slowloris header-read budget. We keep it at 30s (anti-slowloris) and
        // put the configurable idle rotation in the `--idle-timeout` watchdog
        // instead. Consequence: because this timer stays at 30s, when
        // `--idle-timeout` is set > 30, it is effectively capped at ~30s by
        // `header_read_timeout`; values <= 30 are honored by the watchdog, which
        // fires first. Verified empirically by
        // `web_idle_timeout_zero_keeps_connection` (a sub-30s idle wait with the
        // watchdog off keeps the connection alive).
        let mut http = http1::Builder::new();
        http.timer(TokioTimer::new())
            .header_read_timeout(Duration::from_secs(30));
        loop {
            // --max-requests recycling: stop accepting once the cap is reached so
            // the master respawns a fresh worker (bounds memory growth over time).
            // In master mode this runs BEFORE `next()` sends READY (cap-before-READY),
            // so a capped worker exits without being handed one more connection.
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
            // Disable Nagle: responses are typically small and written in one
            // shot, so Nagle interacting with delayed-ACK would add tens of ms of
            // latency to keep-alive request/response round-trips. Best-effort.
            let _ = stream.set_nodelay(true);
            // TLS: read the process-wide acceptor (built pre-fork; `None` on
            // plaintext). The handshake runs INSIDE the connection task below, never
            // in this accept loop, so a slow client handshake (RTT) cannot stall
            // accepting other connections. `https` is threaded into the request path
            // so PHP sees `$_SERVER['HTTPS']`.
            let acceptor = crate::tls::tls_acceptor();
            let https = acceptor.is_some();
            // Per-connection keep-alive rotation state, allocated ONLY when the
            // relevant feature is enabled so the default (both off) hot path keeps
            // the original zero-allocation, zero-bookkeeping behavior. `rotate_on`
            // gates the response counter + close/C3-drain check; `idle_on` gates the
            // last-activity stamps + idle watchdog. When on, each cell lives in this
            // connection's !Send task on the LocalSet, so plain `Rc<Cell<_>>`
            // suffices (no atomics).
            let rotate_on = cfg.max_conn_requests > 0 || cfg.max_requests > 0;
            let idle_on = idle_timeout_secs > 0;
            let conn_served = rotate_on.then(|| Rc::new(Cell::new(0usize)));
            let last_activity = idle_on.then(|| Rc::new(Cell::new(Instant::now())));
            let watchdog_activity = last_activity.clone();
            // `service_fn` is FnMut — called once per request on this connection —
            // so the OUTER closure is non-async and clones the per-connection
            // `Option<Rc<..>>` handles into each returned `async move` block (cloning
            // `None` is free); the Copy config values (`cfg`, `peer`, `addr`, …) are
            // copied in.
            let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                let conn_served = conn_served.clone();
                let last_activity = last_activity.clone();
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
                    let protocol = version_str(req.version());
                    // Captured for the optional access log (method/path are moved into set_request).
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
                    // The body must be fully collected (async) BEFORE the blocking handler
                    // runs, since handler() cannot yield on the current-thread runtime.
                    // Collect with a size cap (0 = unlimited); an over-limit body
                    // short-circuits to 413 without ever running the PHP handler.
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
                    // Keep-alive rotation (only when a rotation feature is enabled):
                    // count this response, then ask hyper to close the connection
                    // (`Connection: close`) when this connection hit its per-connection
                    // cap OR the worker hit its `--max-requests` recycle cap (the C3
                    // drain), so the client reconnects and SO_REUSEPORT re-picks a
                    // worker instead of being cut at exit. With both features off this
                    // block is skipped entirely (`conn_served` is `None`).
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
}

/// Decides whether this keep-alive connection should be closed after the current
/// response, so hyper emits `Connection: close` and the client reconnects (a new
/// source port re-hashes SO_REUSEPORT onto a possibly different worker). Returns
/// true when this connection hit its per-connection cap
/// (`--max-requests-per-connection`), or when the worker itself hit its recycle
/// cap (`--max-requests`) and should drain its keep-alive connections rather than
/// serve them past the cap and cut them at `process::exit` (the C3 drain).
///
/// `conn_served` is this connection's own response count; `served_total` is the
/// worker's process-wide `SERVED` count (passed in because each module owns a
/// separate `SERVED` static). Both caps are disabled at `0`. Shared by the
/// classic and worker-mode serve loops so the predicate lives in one place.
pub(crate) fn should_close_connection(
    conn_served: usize,
    served_total: usize,
    cfg: &WorkerConfig,
) -> bool {
    (cfg.max_conn_requests > 0 && conn_served >= cfg.max_conn_requests)
        || (cfg.max_requests > 0 && served_total >= cfg.max_requests)
}

/// Drives one hyper HTTP/1 connection to completion with an idle-timeout
/// watchdog. `last_activity` is stamped by the connection's service on each
/// request (on entry and after the response); when it has not advanced for
/// `idle`, the connection is gracefully shut down. `graceful_shutdown` finishes
/// any in-flight response before closing the socket (so a slow response is never
/// truncated) and closes an idle keep-alive connection immediately. Shared by
/// both serve loops; only used when `idle_timeout_secs > 0` (the zero path spawns
/// the plain `serve_connection` future with no watchdog).
///
/// This is a hand-rolled select (poll the connection, then the idle timer) via
/// `poll_fn` rather than `tokio::select!`, because the crate does not enable
/// tokio's `macros` feature.
pub(crate) async fn serve_connection_with_idle<I, B, S>(
    conn: http1::Connection<I, S>,
    last_activity: Rc<Cell<Instant>>,
    idle: Duration,
) where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + 'static,
    S: hyper::service::HttpService<hyper::body::Incoming, ResBody = B> + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: hyper::body::Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    use std::future::Future;
    use std::task::Poll;
    // Box::pin both futures so they are movable into the FnMut poll closure and
    // pinned for `poll` / `graceful_shutdown` (both are `!Unpin`).
    let mut conn = Box::pin(conn);
    let mut sleep = Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(
        last_activity.get() + idle,
    )));
    let mut shutting_down = false;
    std::future::poll_fn(move |cx| {
        // Drive the connection first, so an in-flight request/response always makes
        // progress before the idle timer can act.
        if let Poll::Ready(res) = conn.as_mut().poll(cx) {
            let _ = res;
            return Poll::Ready(());
        }
        if !shutting_down && sleep.as_mut().poll(cx).is_ready() {
            let deadline = last_activity.get() + idle;
            if Instant::now() < deadline {
                // A request arrived while we waited and pushed the deadline forward;
                // re-arm the timer to the new deadline and keep serving.
                sleep
                    .as_mut()
                    .reset(tokio::time::Instant::from_std(deadline));
                let _ = sleep.as_mut().poll(cx);
            } else {
                // Genuinely idle past the timeout: begin a graceful shutdown. The
                // connection is still polled above until it finishes, so any
                // in-flight response completes before the socket closes.
                conn.as_mut().graceful_shutdown();
                shutting_down = true;
                if let Poll::Ready(res) = conn.as_mut().poll(cx) {
                    let _ = res;
                    return Poll::Ready(());
                }
            }
        }
        Poll::Pending
    })
    .await;
}

/// gzip-compresses `data`, returning the compressed bytes, or `None` if encoding
/// failed (so the caller leaves the body uncompressed and sets no Content-Encoding).
/// Shared with `crate::worker_mode`.
///
/// Uses `Compression::fast()` (zlib level 1) rather than the default level 6:
/// gzip runs synchronously on the worker's single thread, so a large body at
/// level 6 blocks every other connection on that worker. Level 1 keeps a very
/// similar ratio on HTML/JSON for a fraction of the CPU, which matters far more
/// for tail latency than the marginal extra bytes level 6 would save.
pub(crate) fn gzip_bytes(data: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder =
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
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
