//! Purpose:
//! A minimal loopback HTTP/1.0 server for `ext/curl` codegen fixtures: binds an ephemeral
//! port, then serves `/hello` (`200`/`text/plain`/`hello-curl`), `/status` (`204`),
//! `/redirect` (`302` to `/hello`), `/echo` (a `200` whose body reports the method,
//! headers and request body it received), `/notype` (a `200` with NO `Content-Type`
//! header at all), and `/multipart` (Task 11: a `200` whose body reports the method,
//! `Content-Type`, and a per-part summary of a `multipart/form-data` request) on a
//! background thread for the rest of the test process. Mirrors `tests/codegen/io/streams.rs`'s `spawn_http_server` family (bind on
//! port 0, accept-loop on a detached thread, write a close-framed HTTP/1.0 response)
//! rather than introducing a second HTTP idiom.
//!
//! Called from:
//! - `tests/codegen/curl/easy_http.rs`, `easy_options.rs`, `easy_info.rs`, `multipart.rs`.
//!
//! Key details:
//! - THE REQUEST BODY IS DRAINED, not just the headers. A `POST` fixture whose body the
//!   server never reads makes the client see a connection reset instead of the response,
//!   because closing a socket with unread data queued sends an RST. `Content-Length` is
//!   parsed out of the headers and exactly that many further bytes are read before any
//!   response is written.
//! - `/notype` EXISTS TO PRODUCE A NULL INFO FIELD. `CURLINFO_CONTENT_TYPE` is one of the
//!   few string info fields libcurl really does answer with a NULL `char *` in ordinary
//!   use, and PHP reports that as `false` (typed form) / `null` (array form) rather than
//!   `""`. A response with no `Content-Type` header is the only way to reach it without
//!   inventing a failure.
//! - `/echo` IS WHAT MAKES OPTION FIXTURES REAL. `curl_setopt()` returning `true` only
//!   says libcurl accepted the option; the echo route reports the request that actually
//!   went out, so a fixture can assert on the WIRE rather than on the setter's return
//!   value. Header names are lowercased so assertions do not depend on libcurl's casing.
//! - `/multipart` PARSES THE `multipart/form-data` BODY ITSELF (hand-rolled, no external
//!   crate, matching this whole fixture's existing style), because `/echo`'s raw-body dump
//!   is unreadable for a binary multipart payload and cannot answer "did the upload really
//!   carry field `f`, filename `hello.txt`, content-type `text/plain`, and THIS content".
//!   The boundary is read out of the request's own `Content-Type` header (never assumed),
//!   and the response is one `key=value` line per part field (`part[N].name`/`.filename`/
//!   `.type`/`.size`/`.body`), so a fixture can assert on exact substrings without needing
//!   its own multipart parser.
//! - HTTP/1.0, connection-per-request, no keep-alive: every response either carries an
//!   exact `Content-Length` or is a bodyless `204`, and the socket is dropped right after,
//!   which is how every other loopback fixture in this codebase frames a response for an
//!   HTTP/1.0 client (including libcurl).
//! - TLS is explicitly out of scope here; Wave E's HTTPS fixture lives in `tls_fixture.rs`.
//! - The accept loop runs until the listener errors (which only happens if the test
//!   process is tearing down), so one `LocalHttpServer` can serve more than one connection
//!   across a test's lifetime. Nothing joins the thread — it is intentionally left running
//!   for the rest of the process, the same shape `spawn_http_server` uses.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

/// A running loopback HTTP server plus the port it bound.
pub(crate) struct LocalHttpServer {
    port: u16,
}

impl LocalHttpServer {
    /// Binds `127.0.0.1:0` and starts serving the fixture routes on a background thread.
    pub(crate) fn spawn_hello() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("curl http fixture: bind port");
        let port = listener
            .local_addr()
            .expect("curl http fixture: local addr")
            .port();
        std::thread::spawn(move || loop {
            match listener.accept() {
                Ok((sock, _)) => serve_one(sock, port),
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

/// One parsed request: its method, request-target, header lines, and body.
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// Reads one request (headers plus, when `Content-Length` says so, its body), then writes
/// the matching canned response and drops the connection.
fn serve_one(mut sock: TcpStream, port: u16) {
    let Some(request) = read_request(&mut sock) else {
        return;
    };
    match request.path.as_str() {
        "/hello" => respond(&mut sock, 200, "OK", Some("text/plain"), b"hello-curl", &[]),
        // Two distinct bodies for the multi-interface fixtures: a parallel transfer is
        // only proven if each handle comes back with ITS OWN body, which a single shared
        // route could not show.
        "/a" => respond(&mut sock, 200, "OK", Some("text/plain"), b"body-a", &[]),
        "/b" => respond(&mut sock, 200, "OK", Some("text/plain"), b"body-b", &[]),
        // Deliberately no `Content-Type`, so `CURLINFO_CONTENT_TYPE` is a NULL `char *`.
        "/notype" => respond(&mut sock, 200, "OK", None, b"typeless", &[]),
        "/status" => {
            let _ = sock.write_all(b"HTTP/1.0 204 No Content\r\n\r\n");
        }
        "/redirect" => respond(
            &mut sock,
            302,
            "Found",
            Some("text/plain"),
            b"redirecting",
            &[format!("Location: http://127.0.0.1:{port}/hello")],
        ),
        "/echo" => {
            let mut body = format!("method={}\n", request.method);
            for (name, value) in &request.headers {
                body.push_str(&format!("{name}: {value}\n"));
            }
            body.push_str("body=");
            body.push_str(&String::from_utf8_lossy(&request.body));
            body.push('\n');
            respond(&mut sock, 200, "OK", Some("text/plain"), body.as_bytes(), &[]);
        }
        "/multipart" => {
            let content_type = request
                .headers
                .iter()
                .find(|(name, _)| name == "content-type")
                .map(|(_, value)| value.clone());
            let mut body = format!("method={}\n", request.method);
            body.push_str(&format!(
                "content-type={}\n",
                content_type.as_deref().unwrap_or("")
            ));
            let parts = content_type
                .as_deref()
                .and_then(extract_boundary)
                .map(|boundary| parse_multipart(&request.body, &boundary))
                .unwrap_or_default();
            body.push_str(&format!("parts={}\n", parts.len()));
            for (index, part) in parts.iter().enumerate() {
                body.push_str(&format!("part[{index}].name={}\n", part.name));
                body.push_str(&format!(
                    "part[{index}].filename={}\n",
                    part.filename.as_deref().unwrap_or("")
                ));
                body.push_str(&format!(
                    "part[{index}].type={}\n",
                    part.content_type.as_deref().unwrap_or("")
                ));
                body.push_str(&format!("part[{index}].size={}\n", part.body.len()));
                body.push_str(&format!(
                    "part[{index}].body={}\n",
                    String::from_utf8_lossy(&part.body)
                ));
            }
            respond(&mut sock, 200, "OK", Some("text/plain"), body.as_bytes(), &[]);
        }
        _ => respond(&mut sock, 404, "Not Found", Some("text/plain"), b"missing", &[]),
    }
}

/// Reads the request line, the headers, and exactly `Content-Length` body bytes.
///
/// Draining the body matters: dropping the socket with unread bytes still queued makes
/// the kernel send an RST, which libcurl reports as a failed transfer instead of the
/// response this fixture already wrote.
fn read_request(sock: &mut TcpStream) -> Option<Request> {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while sock.read(&mut byte).unwrap_or(0) == 1 {
        raw.push(byte[0]);
        if raw.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut lines = text.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?.to_string();
    let target = request_line.next()?;
    let path = target.split('?').next().unwrap_or(target).to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if name == "content-length" {
            content_length = value.parse().unwrap_or(0);
        }
        headers.push((name, value));
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 && sock.read_exact(&mut body).is_err() {
        body.clear();
    }
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

/// Writes a close-framed HTTP/1.0 response with an exact `Content-Length`. `content_type`
/// is `None` to omit the header entirely, which is what `/notype` needs.
fn respond(
    sock: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: Option<&str>,
    body: &[u8],
    extra_headers: &[String],
) {
    let mut header = format!("HTTP/1.0 {status} {reason}\r\n");
    if let Some(content_type) = content_type {
        header.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    header.push_str(&format!("Content-Length: {}\r\n", body.len()));
    for line in extra_headers {
        header.push_str(line);
        header.push_str("\r\n");
    }
    header.push_str("\r\n");
    let _ = sock.write_all(header.as_bytes());
    let _ = sock.write_all(body);
}

/// One parsed `multipart/form-data` part: its `Content-Disposition` field name and
/// (optional) filename, its own `Content-Type` if it carried one, and its raw body bytes.
struct MultipartPart {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

/// Extracts the `boundary=` value from a `Content-Type: multipart/form-data; boundary=…`
/// header, handling both the quoted and bare forms libcurl and other clients use.
fn extract_boundary(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let index = lower.find("boundary=")?;
    let rest = &content_type[index + "boundary=".len()..];
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else {
        Some(rest.split(';').next().unwrap_or(rest).trim().to_string())
    }
}

/// Returns the index of the first occurrence of `needle` in `haystack`, or `None`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Parses a `multipart/form-data` body into its parts, given the boundary token from the
/// request's own `Content-Type` header.
///
/// Every occurrence of `--boundary` (the delimiter line, without its own leading/trailing
/// CRLF) is located first; the byte range between each CONSECUTIVE pair of occurrences is
/// exactly one part's framing (headers, a blank line, the part body, and the CRLF
/// immediately before the next delimiter) — including the LAST pair, whose second
/// occurrence is the closing `--boundary--` delimiter, which this function never needs to
/// distinguish from an ordinary one because only what comes BEFORE it is ever read.
fn parse_multipart(body: &[u8], boundary: &str) -> Vec<MultipartPart> {
    let delimiter = format!("--{boundary}").into_bytes();
    let mut occurrences = Vec::new();
    let mut search_from = 0usize;
    while let Some(relative) = find_bytes(&body[search_from..], &delimiter) {
        let absolute = search_from + relative;
        occurrences.push(absolute);
        search_from = absolute + delimiter.len();
    }

    let mut parts = Vec::new();
    for window in occurrences.windows(2) {
        let start = window[0] + delimiter.len();
        let end = window[1];
        if start > end {
            continue;
        }
        let mut segment = &body[start..end];
        if let Some(stripped) = segment.strip_prefix(b"\r\n") {
            segment = stripped;
        }
        if let Some(stripped) = segment.strip_suffix(b"\r\n") {
            segment = stripped;
        }
        let Some(header_end) = find_bytes(segment, b"\r\n\r\n") else {
            continue;
        };
        let header_block = &segment[..header_end];
        let content = &segment[header_end + 4..];

        let mut name = String::new();
        let mut filename = None;
        let mut content_type = None;
        let header_text = String::from_utf8_lossy(header_block).into_owned();
        for line in header_text.split("\r\n") {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-disposition:") {
                name = extract_quoted(line, "name=").unwrap_or_default();
                filename = extract_quoted(line, "filename=");
            } else if lower.starts_with("content-type:") {
                content_type = line.splitn(2, ':').nth(1).map(|value| value.trim().to_string());
            }
        }
        parts.push(MultipartPart {
            name,
            filename,
            content_type,
            body: content.to_vec(),
        });
    }
    parts
}

/// Extracts a `key="value"` quoted attribute from a header line (e.g. `name="f"` out of a
/// `Content-Disposition` line), case-insensitively on the key.
fn extract_quoted(line: &str, key: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let index = lower.find(key)?;
    let rest = line[index + key.len()..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
