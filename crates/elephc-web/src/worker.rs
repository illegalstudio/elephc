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

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

use crate::request_state;

/// Builds a listening std::net::TcpListener with SO_REUSEPORT set, bound to `addr`.
fn reuseport_listener(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&addr.into())?;
    sock.listen(1024)?;
    Ok(sock.into())
}

/// Serves HTTP forever on `listen` (host:port) in this worker process.
/// Builds a current-thread tokio runtime and loops accepting connections,
/// serving each with the PHP handler. `max_body` caps the request body in bytes
/// (`0` = unlimited); an over-limit body short-circuits to HTTP 413.
pub fn serve(listen: &str, handler: extern "C" fn(), max_body: usize) {
    let addr: SocketAddr = listen.parse().expect("invalid --listen host:port");
    let std_listener = reuseport_listener(addr).expect("failed to bind SO_REUSEPORT socket");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    rt.block_on(async move {
        let listener = TcpListener::from_std(std_listener).expect("from_std");
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let io = TokioIo::new(stream);
            // Serve this connection on the current thread; the blocking handler
            // call below holds the thread, serializing requests in this worker.
            if let Err(_e) = http1::Builder::new()
                .serve_connection(io, service_fn(move |req: Request<hyper::body::Incoming>| async move {
                    let method = req.method().as_str().to_string();
                    let uri = req.uri().to_string();
                    let path = req.uri().path().to_string();
                    let query = req.uri().query().unwrap_or("").to_string();
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
                    request_state::set_request(method, uri, path, query, headers, body);
                    let resp_body = run_handler(handler);
                    let status = request_state::take_status();
                    let mut builder = Response::builder().status(status);
                    for (name, value) in request_state::take_headers() {
                        builder = builder.header(name, value);
                    }
                    let response = builder
                        .body(Full::new(Bytes::from(resp_body)))
                        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))));
                    Ok::<_, Infallible>(response)
                }))
                .await
            {
                // Connection-level errors are non-fatal to the worker.
            }
        }
    });
}

/// Runs the PHP handler for one request and returns the captured response body.
fn run_handler(handler: extern "C" fn()) -> Vec<u8> {
    request_state::set_capture(true);
    request_state::clear_body();
    request_state::reset_response();
    handler();
    request_state::take_body()
}
