//! Purpose:
//! A minimal loopback HTTP/1.0 server for `ext/curl` codegen fixtures: binds an ephemeral
//! port, then serves `GET /hello` (`200`/`text/plain`/`hello-curl`) and `GET /status`
//! (`204`) on a background thread for the rest of the test process. Mirrors
//! `tests/codegen/io/streams.rs`'s `spawn_http_server` family (bind on port 0, accept-loop
//! on a detached thread, drain the request through the blank line that ends the headers,
//! write a close-framed HTTP/1.0 response) rather than introducing a second HTTP idiom.
//!
//! Called from:
//! - `tests/codegen/curl/easy_http.rs`.
//!
//! Key details:
//! - HTTP/1.0, connection-per-request, no keep-alive: every response either carries an
//!   exact `Content-Length` (`/hello`) or is a bodyless `204` (`/status`), and the socket is
//!   dropped right after, which is how every other loopback fixture in this codebase frames
//!   a response for an HTTP/1.0 client (including libcurl).
//! - TLS is explicitly out of scope here (`.superpowers/sdd/php-curl-family/task-7-brief.md`
//!   points HTTPS at Task 8 Wave E's own fixture); this server is plaintext only.
//! - The accept loop runs until the listener errors (which only happens if the test process
//!   is tearing down), so one `LocalHttpServer` can serve more than one connection across a
//!   test's lifetime. Nothing joins the thread — it is intentionally left running for the
//!   rest of the process, the same shape `spawn_http_server` uses.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// A running loopback HTTP server plus the port it bound.
pub(crate) struct LocalHttpServer {
    port: u16,
}

impl LocalHttpServer {
    /// Binds `127.0.0.1:0` and starts serving `/hello` and `/status` on a background
    /// thread.
    pub(crate) fn spawn_hello() -> Self {
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).expect("curl http fixture: bind port");
        let port = listener
            .local_addr()
            .expect("curl http fixture: local addr")
            .port();
        std::thread::spawn(move || loop {
            match listener.accept() {
                Ok((sock, _)) => serve_one(sock),
                Err(_) => return,
            }
        });
        Self { port }
    }

    /// Builds `http://127.0.0.1:<port><path>` for this server.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

/// Reads one request up to the blank line ending the headers, then writes the matching
/// canned response and drops the connection.
fn serve_one(mut sock: TcpStream) {
    let mut req = Vec::new();
    let mut byte = [0u8; 1];
    while sock.read(&mut byte).unwrap_or(0) == 1 {
        req.push(byte[0]);
        if req.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    match request_path(&req).as_deref() {
        Some("/hello") => {
            let body = b"hello-curl";
            let header = format!(
                "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(header.as_bytes());
            let _ = sock.write_all(body);
        }
        Some("/status") => {
            let _ = sock.write_all(b"HTTP/1.0 204 No Content\r\n\r\n");
        }
        _ => {
            let _ = sock.write_all(b"HTTP/1.0 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    }
}

/// Extracts the request-target from an HTTP request line (`GET /hello HTTP/1.1\r\n...`),
/// ignoring any query string.
fn request_path(req: &[u8]) -> Option<String> {
    let line_end = req.iter().position(|&b| b == b'\r')?;
    let line = std::str::from_utf8(&req[..line_end]).ok()?;
    let mut parts = line.split(' ');
    let _method = parts.next()?;
    let target = parts.next()?;
    Some(target.split('?').next().unwrap_or(target).to_string())
}
