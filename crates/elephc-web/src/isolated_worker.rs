//! Purpose:
//! Isolated per-worker HTTP serving: prestart a threadless handler broker, build a
//! SO_REUSEPORT listener, and stream isolated PHP responses through Tokio.
//!
//! Called from:
//! - `crate::server::spawn_worker` for pool- and request-isolated children.
//!
//! Key details:
//! - PHP runs only in persistent-pool or disposable-request handler children.
//! - Connection tasks perform async IPC and never fork or consume a blocking pool.
//! - SO_REUSEPORT lets every worker bind the same port; the kernel balances.

use std::convert::Infallible;
use std::fmt;
use std::io::Write;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Limited};
use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

use crate::handler_broker::{BrokerMode, HandlerBroker, HandlerResponse, PrestartedBroker};
use crate::handler_ipc::{HandlerRequest, ResponseFrame, MAX_REQUEST_BODY_BYTES};
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

/// Records one completed isolated handler and broadcasts a graceful recycle at the quota.
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

/// Per-worker serving configuration (all `Copy`, so it survives `fork` and moves
/// into the connection tasks freely).
#[derive(Clone, Copy)]
pub struct WorkerConfig {
    /// Max request body in bytes; `0` = unlimited (over-limit → HTTP 413).
    pub max_body: usize,
    /// Body receive deadline in seconds; `0` = unlimited (deadline → HTTP 408).
    pub body_read_secs: u64,
    /// Response backpressure deadline in seconds; `0` = unlimited.
    pub response_write_secs: u64,
    /// Recycle the worker after this many requests; `0` = never.
    pub max_requests: usize,
    /// Log one line per request to stderr.
    pub access_log: bool,
    /// Per-request handler time limit in seconds; `0` = no limit.
    pub max_exec_secs: u32,
    /// gzip the response body when the client sent `Accept-Encoding: gzip`.
    pub gzip: bool,
    /// Persistent-pool or disposable-request handler lifecycle.
    pub handler_mode: BrokerMode,
    /// Maximum handler processes owned by this worker's broker.
    pub handler_concurrency: usize,
    /// Requests served by one pool child before planned replacement; `0` = never.
    pub max_handler_requests: usize,
}

/// Minimum response size (bytes) worth gzip-compressing; below this the framing
/// overhead outweighs the savings.
const GZIP_MIN_LEN: usize = 256;
/// Number of body frames queued per response before child-side backpressure applies.
const RESPONSE_CHANNEL_CAPACITY: usize = 8;

/// Ignores process-level SIGPIPE so closed IPC/TCP peers surface as I/O errors.
fn ignore_sigpipe() {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = libc::SIG_IGN;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGPIPE, &action, std::ptr::null_mut());
    }
}

/// Response state returned once an isolated PHP handler commits its headers.
struct HandlerResult {
    status: u16,
    headers: Vec<(String, String)>,
    body: WebBody,
}

/// Error surfaced to Hyper when a committed handler response becomes incomplete.
#[derive(Debug)]
struct ResponseStreamError;

impl fmt::Display for ResponseStreamError {
    /// Describes why Hyper must abort the HTTP response instead of ending it cleanly.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("handler response ended before its completion frame")
    }
}

impl std::error::Error for ResponseStreamError {}

/// Result of waiting for Hyper's bounded response queue to accept one frame.
enum SendFrameResult {
    Sent,
    Closed,
    TimedOut,
}

/// Hyper body backed by a bounded handler-response channel.
struct WebBody {
    receiver: mpsc::Receiver<Frame<Bytes>>,
    exact_len: Option<u64>,
    aborted: Arc<AtomicBool>,
    abort_reported: bool,
}

impl Body for WebBody {
    type Data = Bytes;
    type Error = ResponseStreamError;

    /// Polls the next bounded handler chunk or the end of the response stream.
    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match this.receiver.poll_recv(context) {
            Poll::Ready(Some(frame)) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(None)
                if this.aborted.load(Ordering::Acquire) && !this.abort_reported =>
            {
                this.abort_reported = true;
                Poll::Ready(Some(Err(ResponseStreamError)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    /// Reports an unknown streaming size so Hyper selects chunked framing when needed.
    fn size_hint(&self) -> SizeHint {
        match self.exact_len {
            Some(length) => SizeHint::with_exact(length),
            None => SizeHint::default(),
        }
    }
}

/// Serves HTTP on `listen` (host:port) in this worker process. Builds a
/// current-thread tokio runtime and loops accepting connections, serving each
/// with the PHP handler per `WorkerConfig`.
pub fn serve(
    listen: &str,
    handler: extern "C" fn(),
    cfg: WorkerConfig,
    inherited_listener: Option<std::os::fd::RawFd>,
) {
    let WorkerConfig {
        max_body,
        body_read_secs,
        response_write_secs,
        max_requests,
        access_log,
        max_exec_secs,
        gzip,
        handler_mode,
        handler_concurrency,
        max_handler_requests,
    } = cfg;
    let addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("elephc-web: invalid --listen address {:?}", listen);
            std::process::exit(1);
        }
    };
    ignore_sigpipe();
    unsafe { crate::session::elephc_web_session_reset() };
    upload_progress::initialize_config();
    // This process is still single-threaded and owns no Tokio resources here.
    // The broker inherits only stable compiler/runtime state, then becomes the
    // sole process allowed to own and fork handler children.
    let prestarted_broker = match PrestartedBroker::start(
        handler,
        max_exec_secs,
        handler_mode,
        handler_concurrency,
        max_handler_requests,
    ) {
        Ok(broker) => broker,
        Err(error) => {
            eprintln!("elephc-web: failed to start handler broker: {error}");
            std::process::exit(1);
        }
    };
    // A descriptor handed down by the master is already bound and listening: adopting it is
    // what makes every worker accept from ONE queue on platforms where SO_REUSEPORT does not
    // distribute. Without one, each worker binds its own SO_REUSEPORT socket as before.
    let std_listener = match inherited_listener {
        Some(fd) => match unsafe { crate::shared_listener::adopt(fd) } {
            Ok(l) => l,
            Err(e) => {
                eprintln!("elephc-web: failed to adopt the inherited listener: {}", e);
                std::process::exit(1);
            }
        },
        None => match reuseport_listener(addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("elephc-web: failed to bind {}: {}", addr, e);
                std::process::exit(1);
            }
        },
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    // A LocalSet lets each connection run as its own !Send task on this single
    // thread, while handler execution remains isolated behind async Unix streams.
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let broker = match prestarted_broker.into_async() {
            Ok(broker) => broker,
            Err(error) => {
                eprintln!("elephc-web: failed to register handler broker: {error}");
                std::process::exit(1);
            }
        };
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
            let broker = broker.clone();
            let request_recycle = recycle.clone();
            let mut connection_recycle = recycle.subscribe();
            connections.push(tokio::task::spawn_local(async move {
                let connection = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(30))
                    .serve_connection(io, service_fn(move |req: Request<hyper::body::Incoming>| {
                        let broker = broker.clone();
                        let request_recycle = request_recycle.clone();
                        async move {
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
                    //
                    // For a tracked `session.upload_progress` multipart upload, the body
                    // is drained frame-by-frame so progress can be written to the session
                    // file as bytes arrive, while still buffering the complete body the
                    // handler sees. Non-tracked requests keep the simple fast path.
                    let collect_body = async {
                        if let Some(mut tracker) = upload_progress::begin(&headers, &query) {
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
                    let body = match collected {
                        Err(()) => {
                            return Ok::<_, Infallible>(fixed_response(408, b"Request Timeout"));
                        }
                        Ok(Ok(body)) => body,
                        Ok(Err(_)) => {
                            return Ok::<_, Infallible>(fixed_response(413, b"Payload Too Large"));
                        }
                    };
                    if body.len() > MAX_REQUEST_BODY_BYTES {
                        return Ok::<_, Infallible>(fixed_response(413, b"Payload Too Large"));
                    }
                    let request = HandlerRequest {
                        method,
                        uri,
                        path,
                        query,
                        headers,
                        body,
                        remote_addr: peer.ip().to_string(),
                        remote_port: peer.port(),
                        server_addr: addr.ip().to_string(),
                        server_port: addr.port(),
                        protocol,
                    };
                    let handler_result =
                        run_handler_isolated(
                            &broker,
                            request,
                            accepts_gzip,
                            response_write_secs,
                            max_requests,
                            &request_recycle,
                        )
                        .await;
                    let status = handler_result.status;
                    let mut builder = Response::builder().status(status);
                    for (name, value) in handler_result.headers {
                        builder = builder.header(name, value);
                    }
                    let response = builder
                        .body(handler_result.body)
                        .unwrap_or_else(|_| fixed_response(500, b"Internal Server Error"));
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
    crate::handler_broker::finish_worker_recycle();
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

/// Dispatches a request to the prestarted broker and prepares its streaming body.
async fn run_handler_isolated(
    broker: &HandlerBroker,
    request: HandlerRequest,
    accepts_gzip: bool,
    response_write_secs: u64,
    max_requests: usize,
    recycle: &watch::Sender<bool>,
) -> HandlerResult {
    let mut response = match broker.dispatch(request).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("elephc-web: handler dispatch failed: {error}");
            record_completed_request(max_requests, recycle);
            return handler_failure_response();
        }
    };
    let mut headers = std::mem::take(&mut response.start.headers);
    let status = response.start.status;
    let gzip_candidate = accepts_gzip
        && !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-encoding"));
    if gzip_candidate {
        let mut prefix = Vec::new();
        loop {
            match response.next_frame().await {
                Ok(ResponseFrame::Chunk(chunk)) => {
                    prefix.extend_from_slice(&chunk);
                    if prefix.len() >= GZIP_MIN_LEN {
                        headers.retain(|(name, _)| !name.eq_ignore_ascii_case("content-length"));
                        headers.push(("content-encoding".to_string(), "gzip".to_string()));
                        return HandlerResult {
                            status,
                            headers,
                            body: stream_body(
                                response,
                                prefix,
                                true,
                                response_write_secs,
                                max_requests,
                                recycle.clone(),
                            ),
                        };
                    }
                }
                Ok(ResponseFrame::End) => {
                    record_completed_request(max_requests, recycle);
                    return HandlerResult {
                        status,
                        headers,
                        body: body_from_bytes(prefix),
                    };
                }
                Ok(ResponseFrame::Start(_)) | Err(_) => {
                    record_completed_request(max_requests, recycle);
                    return HandlerResult {
                        status,
                        headers,
                        body: aborted_body(prefix),
                    };
                }
            }
        }
    }
    match response.next_frame().await {
        Ok(ResponseFrame::Chunk(chunk)) => HandlerResult {
            status,
            headers,
            body: stream_body(
                response,
                chunk,
                false,
                response_write_secs,
                max_requests,
                recycle.clone(),
            ),
        },
        Ok(ResponseFrame::End) => {
            record_completed_request(max_requests, recycle);
            HandlerResult {
                status,
                headers,
                body: body_from_bytes(Vec::new()),
            }
        }
        Ok(ResponseFrame::Start(_)) | Err(_) => {
            record_completed_request(max_requests, recycle);
            HandlerResult {
                status,
                headers,
                body: aborted_body(Vec::new()),
            }
        }
    }
}

/// Produces the bounded 500 response used when a handler dies before output.
fn handler_failure_response() -> HandlerResult {
    HandlerResult {
        status: 500,
        headers: Vec::new(),
        body: body_from_bytes(b"Internal Server Error".to_vec()),
    }
}

/// Builds a fixed error response using the same body type as streamed handlers.
fn fixed_response(status: u16, body: &'static [u8]) -> Response<WebBody> {
    Response::builder()
        .status(status)
        .body(body_from_bytes(body.to_vec()))
        .unwrap_or_else(|_| Response::new(body_from_bytes(Vec::new())))
}

/// Creates a completed bounded body containing at most one in-memory byte vector.
fn body_from_bytes(bytes: Vec<u8>) -> WebBody {
    let (sender, receiver) = mpsc::channel(1);
    let exact_len = bytes.len() as u64;
    if !bytes.is_empty() {
        let _ = sender.try_send(Frame::data(Bytes::from(bytes)));
    }
    drop(sender);
    WebBody {
        receiver,
        exact_len: Some(exact_len),
        aborted: Arc::new(AtomicBool::new(false)),
        abort_reported: false,
    }
}

/// Creates an incomplete streaming body that ends with an explicit Hyper error.
fn aborted_body(prefix: Vec<u8>) -> WebBody {
    let (sender, receiver) = mpsc::channel(1);
    if !prefix.is_empty() {
        let _ = sender.try_send(Frame::data(Bytes::from(prefix)));
    }
    drop(sender);
    WebBody {
        receiver,
        exact_len: None,
        aborted: Arc::new(AtomicBool::new(true)),
        abort_reported: false,
    }
}

/// Starts a bounded pump from one handler channel into Hyper's response body.
fn stream_body(
    response: HandlerResponse,
    prefix: Vec<u8>,
    gzip: bool,
    response_write_secs: u64,
    max_requests: usize,
    recycle: watch::Sender<bool>,
) -> WebBody {
    let (sender, receiver) = mpsc::channel(RESPONSE_CHANNEL_CAPACITY);
    let aborted = Arc::new(AtomicBool::new(false));
    if gzip || prefix.is_empty() {
        tokio::task::spawn_local(pump_response(
            response,
            prefix,
            gzip,
            response_write_secs,
            sender,
            Arc::clone(&aborted),
            max_requests,
            recycle.clone(),
        ));
    } else {
        let _ = sender.try_send(Frame::data(Bytes::from(prefix)));
        tokio::task::spawn_local(pump_response(
            response,
            Vec::new(),
            false,
            response_write_secs,
            sender,
            Arc::clone(&aborted),
            max_requests,
            recycle,
        ));
    }
    WebBody {
        receiver,
        exact_len: None,
        aborted,
        abort_reported: false,
    }
}

/// Pumps response chunks with backpressure, optionally through streaming gzip.
async fn pump_response(
    mut response: HandlerResponse,
    prefix: Vec<u8>,
    gzip: bool,
    response_write_secs: u64,
    sender: mpsc::Sender<Frame<Bytes>>,
    aborted: Arc<AtomicBool>,
    max_requests: usize,
    recycle: watch::Sender<bool>,
) {
    let mut encoder = gzip.then(|| {
        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default())
    });
    if !prefix.is_empty()
        && !matches!(
            send_output(&sender, &mut encoder, &prefix, response_write_secs).await,
            SendFrameResult::Sent
        )
    {
        aborted.store(true, Ordering::Release);
        record_completed_request(max_requests, &recycle);
        return;
    }
    loop {
        match response.next_frame().await {
            Ok(ResponseFrame::Chunk(chunk)) => {
                if !matches!(
                    send_output(&sender, &mut encoder, &chunk, response_write_secs).await,
                    SendFrameResult::Sent
                ) {
                    aborted.store(true, Ordering::Release);
                    break;
                }
            }
            Ok(ResponseFrame::End) => {
                if let Some(encoder) = encoder.take() {
                    if let Ok(final_bytes) = encoder.finish() {
                        if !matches!(
                            send_frame(&sender, final_bytes, response_write_secs).await,
                            SendFrameResult::Sent
                        ) {
                            aborted.store(true, Ordering::Release);
                        }
                    } else {
                        aborted.store(true, Ordering::Release);
                    }
                }
                break;
            }
            Ok(ResponseFrame::Start(_)) | Err(_) => {
                aborted.store(true, Ordering::Release);
                break;
            }
        }
    }
    record_completed_request(max_requests, &recycle);
}

/// Sends plaintext or newly-produced gzip bytes for one handler chunk.
async fn send_output(
    sender: &mpsc::Sender<Frame<Bytes>>,
    encoder: &mut Option<flate2::write::GzEncoder<Vec<u8>>>,
    bytes: &[u8],
    response_write_secs: u64,
) -> SendFrameResult {
    if let Some(encoder) = encoder {
        if encoder.write_all(bytes).is_err() || encoder.flush().is_err() {
            return SendFrameResult::Closed;
        }
        let compressed = std::mem::take(encoder.get_mut());
        send_frame(sender, compressed, response_write_secs).await
    } else {
        send_frame(sender, bytes.to_vec(), response_write_secs).await
    }
}

/// Enqueues one non-empty Hyper data frame, respecting client backpressure.
async fn send_frame(
    sender: &mpsc::Sender<Frame<Bytes>>,
    bytes: Vec<u8>,
    response_write_secs: u64,
) -> SendFrameResult {
    if bytes.is_empty() {
        return SendFrameResult::Sent;
    }
    let send = sender.send(Frame::data(Bytes::from(bytes)));
    if response_write_secs == 0 {
        return if send.await.is_ok() {
            SendFrameResult::Sent
        } else {
            SendFrameResult::Closed
        };
    }
    match tokio::time::timeout(Duration::from_secs(response_write_secs), send).await {
        Ok(Ok(())) => SendFrameResult::Sent,
        Ok(Err(_)) => SendFrameResult::Closed,
        Err(_) => SendFrameResult::TimedOut,
    }
}
