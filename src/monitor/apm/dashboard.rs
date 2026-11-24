//! Purpose:
//! The server behind `monitor --apm --serve <host:port>`: connections, request
//! heads, the paths, a server-sent event per new window and a profile capture
//! on request. What each path answers is built in `api`.
//!
//! Called from:
//! - `apm::run` / `apm::collect`, which start it beside the collector, or alone
//!   to browse a store.
//!
//! Key details:
//! - std only: one thread per connection, which is plenty for the handful of
//!   browsers and scrapers a dashboard has, and a bounded number of them.
//! - Every connection gives its slot back however it ends, a request head must
//!   arrive whole within a deadline, a write to a client that stopped reading
//!   times out, and an event stream ends after a while (the browser reconnects
//!   at once): a slot is never held by a client that is gone (review).
//! - The page fetches nothing from the network; charts are drawn in inline SVG.
//! - Bind a loopback address unless the routes and timings are meant to be
//!   public: like the live call graph, this server has no authentication.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::api::{alerts_json, metrics_text, page_html, series_json, traces_json};
use super::Shared;

/// How long a profile capture lets the service run under the sampler.
const CAPTURE_SAMPLING: Duration = Duration::from_secs(3);
/// How long an idle event stream waits before sending a keep-alive comment.
const KEEPALIVE: Duration = Duration::from_secs(15);
/// How long one event stream lasts before the server ends it; the page's
/// `EventSource` reconnects after `STREAM_RETRY_MS`.
const STREAM_LIFETIME: Duration = Duration::from_secs(300);
/// What the event stream tells the browser to wait before reconnecting, ms.
const STREAM_RETRY_MS: u64 = 1_000;
/// How long the whole request head may take to arrive.
const HEAD_DEADLINE: Duration = Duration::from_secs(10);
/// How long one write may block on a client that does not read.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// One connection's slot, given back when the connection's thread ends —
/// returned, failed or panicked alike. A slot given back only on return was
/// lost for good to a panicking request, and 64 lost slots answered every
/// later request 503 (review).
pub(super) struct Slot(pub(super) Arc<AtomicUsize>);

impl Drop for Slot {
    /// Gives the slot back.
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Binds `addr` and serves the dashboard on a background thread.
pub(crate) fn serve(
    addr: &str,
    shared: Shared,
) -> std::io::Result<(SocketAddr, std::thread::JoinHandle<()>)> {
    let listener = TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    let open = Arc::new(AtomicUsize::new(0));
    let handle = std::thread::Builder::new()
        .name("elephc-apm-dashboard".to_string())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                // A bounded number of connections: each holds a thread, and an
                // event stream holds it for as long as the page stays open.
                if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    let _ = respond(&mut stream, "503 Service Unavailable", "text/plain", b"too many connections\n");
                    continue;
                }
                open.fetch_add(1, Ordering::Relaxed);
                let slot = Slot(open.clone());
                let shared = shared.clone();
                // A thread that cannot be spawned drops its closure, and the
                // slot with it.
                let _ = std::thread::Builder::new()
                    .name("elephc-apm-request".to_string())
                    .spawn(move || {
                        let _slot = slot;
                        let _ = handle(stream, &shared, local);
                    });
            }
        })?;
    Ok((local, handle))
}

/// Connections served at once; more are answered 503.
const MAX_CONNECTIONS: usize = 64;
/// Longest request head read, in bytes: line, headers and all.
const MAX_HEAD_BYTES: u64 = 16 * 1024;
/// Most header lines read.
const MAX_HEADERS: usize = 100;

/// A reader that fails once its deadline has passed: the per-read timeout
/// alone let a client send its head a byte every few seconds and keep its
/// connection's slot for as long as it liked.
pub(super) struct Deadline<R> {
    pub(super) inner: R,
    pub(super) until: Instant,
}

impl<R: std::io::Read> std::io::Read for Deadline<R> {
    /// Reads from the inner reader, or fails with `TimedOut` past the deadline.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if Instant::now() >= self.until {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "request head too slow"));
        }
        self.inner.read(buf)
    }
}

/// What the server reads of a request.
struct RequestHead {
    method: String,
    target: String,
    host: Option<String>,
    origin: Option<String>,
}

/// Reads the request line and headers, bounded in size and count.
fn read_head(stream: &TcpStream) -> std::io::Result<Option<RequestHead>> {
    use std::io::Read as _;
    let deadline = Deadline { inner: stream, until: Instant::now() + HEAD_DEADLINE };
    let mut reader = BufReader::new(deadline.take(MAX_HEAD_BYTES));
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 || !line.ends_with('\n') {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };
    let mut head = RequestHead { method: method.to_string(), target: target.to_string(), host: None, origin: None };
    for _ in 0..=MAX_HEADERS {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 || !header.ends_with('\n') {
            return Ok(None);
        }
        if header == "\r\n" || header == "\n" {
            return Ok(Some(head));
        }
        if let Some((name, value)) = header.split_once(':') {
            let value = value.trim().to_string();
            match name.trim().to_ascii_lowercase().as_str() {
                "host" => head.host = Some(value),
                "origin" => head.origin = Some(value),
                _ => {}
            }
        }
    }
    Ok(None)
}

/// Whether a request's `Host` names this server, when it listens on loopback.
///
/// A page on another site cannot read a loopback server — unless it rebinds
/// its own name to 127.0.0.1, after which the browser sends that name as the
/// Host. Accepting only loopback names closes that door. A server bound to a
/// public address is reached under names it cannot know, so it does not check.
pub(crate) fn host_is_allowed(host: Option<&str>, local: SocketAddr) -> bool {
    if !local.ip().is_loopback() {
        return true;
    }
    let Some(host) = host else {
        return false;
    };
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(':') => (name, port),
        _ => return false,
    };
    port.parse::<u16>().ok() == Some(local.port())
        && matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Answers one HTTP request.
fn handle(mut stream: TcpStream, shared: &Shared, local: SocketAddr) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let Some(head) = read_head(&stream)? else {
        return respond(&mut stream, "400 Bad Request", "text/plain", b"bad request\n");
    };
    if !host_is_allowed(head.host.as_deref(), local) {
        return respond(&mut stream, "403 Forbidden", "text/plain", b"unexpected Host\n");
    }
    let target = head.target.clone();
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    // Capturing a profile arms the service's sampler, and resetting its route
    // table empties it: state changes, so they take a POST from this page's
    // own origin, never a GET a link or an image on another site could send.
    if path == "/api/capture" || path == "/api/reset-routes" {
        let same_origin = head.origin.as_deref().is_none_or(|origin| {
            head.host.as_deref().is_some_and(|host| origin == format!("http://{host}"))
        });
        if head.method != "POST" {
            return respond(&mut stream, "405 Method Not Allowed", "text/plain", b"POST only\n");
        }
        if !same_origin {
            return respond(&mut stream, "403 Forbidden", "text/plain", b"cross-origin request refused\n");
        }
    } else if head.method != "GET" {
        return respond(&mut stream, "405 Method Not Allowed", "text/plain", b"GET only\n");
    }
    match path {
        "/" => {
            let trace = query_param(query, "trace").and_then(|id| id.parse::<u64>().ok());
            let page = page_html(shared, trace);
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", page.as_bytes())
        }
        "/api/capture" => match capture_profile(shared) {
            Ok(()) => respond(&mut stream, "200 OK", "text/plain", b"captured\n"),
            Err(error) => respond(&mut stream, "502 Bad Gateway", "text/plain", error.as_bytes()),
        },
        "/api/reset-routes" => match reset_routes(shared) {
            Ok(answer) => respond(&mut stream, "200 OK", "text/plain", answer.as_bytes()),
            Err(error) => respond(&mut stream, "502 Bad Gateway", "text/plain", error.as_bytes()),
        },
        "/api/series" => {
            let body = series_json(shared, query).to_string();
            respond(&mut stream, "200 OK", "application/json", body.as_bytes())
        }
        "/api/stream" => stream_events(stream, shared),
        "/api/alerts" => {
            let body = alerts_json(shared).to_string();
            respond(&mut stream, "200 OK", "application/json", body.as_bytes())
        }
        "/api/traces" => {
            let body = traces_json(shared, query).to_string();
            respond(&mut stream, "200 OK", "application/json", body.as_bytes())
        }
        "/metrics" => {
            let body = metrics_text(shared);
            respond(&mut stream, "200 OK", "text/plain; version=0.0.4", body.as_bytes())
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found\n"),
    }
}

/// Asks the service for its sampled profile and keeps it as the newest frame.
///
/// Asking is what starts the service's sampler, exactly as `monitor <service>`
/// does: the button is the operator asking, so a service nobody profiles keeps
/// its sampler dormant however long the dashboard stays open.
fn capture_profile(shared: &Shared) -> Result<(), String> {
    let (target, key) = shared
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .capture
        .clone()
        .ok_or("this dashboard browses a stored history; there is no service to profile")?;
    // The first ask is what starts the sampler, so its answer holds only the
    // few samples of the server's short warm-up. Ask, let the service run under
    // the sampler for a few seconds, and keep the second answer.
    super::super::fetch_service_answer(&target, &key, elephc_probe::endpoint::WANT_SAMPLED, false)?;
    std::thread::sleep(CAPTURE_SAMPLING);
    let folded = super::super::fetch_service_answer(&target, &key, elephc_probe::endpoint::WANT_SAMPLED, false)?;
    let display = super::super::folded_text_to_display(&folded);
    if display.is_empty() {
        return Err("no samples yet: the sampler starts when asked, so capture again — if it stays empty, the service is idle".to_string());
    }
    let graph = super::super::build_call_graph(&display);
    let t_ms = super::super::epoch_millis();
    let mut history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    history.profiles.push_back((t_ms, graph.clone()));
    while history.profiles.len() > 10 {
        history.profiles.pop_front();
    }
    // Dated in the store too, so it is found again by its period after the page's ten
    // newest have moved on. Written after the lock is let go.
    let store = history.store.clone();
    let service = history.target.clone();
    drop(history);
    if let Some(store) = store {
        let t = t_ms as u64;
        let record = super::captures::CaptureRecord {
            id: super::captures::next_capture_id(t),
            source: service.clone(),
            t,
            kind: "profile".to_string(),
            route: format!("sampled profile of {service}"),
            status: None,
            duration_ns: CAPTURE_SAMPLING.as_nanos() as u64,
            functions: graph.nodes.len(),
            queries: graph.queries.iter().map(|(_, n)| n).sum(),
        };
        store.append_capture(&record, &graph)?;
    }
    Ok(())
}

/// Empties the service's route table, as `--reset-routes` does, and returns
/// what it answered. The collector reads the new run of counters as a
/// restart at its next poll; what it stored before keeps the old routes.
fn reset_routes(shared: &Shared) -> Result<String, String> {
    let (target, key) = shared
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .capture
        .clone()
        .ok_or("this dashboard browses a stored history; there is no service to reset")?;
    super::collector::reset_service_routes(&target, &key)
}

/// Writes a complete response.
fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Holds the connection open and sends one event per new window, so the page
/// redraws when there is something new rather than on a timer of its own.
///
/// For `STREAM_LIFETIME` at most: a client that vanished without a word (a
/// laptop put to sleep, a dropped NAT entry) is noticed only when a write
/// fails, which a few bytes every fifteen seconds take long to cause. Ending
/// the stream frees its slot; a page still open reconnects within a second.
fn stream_events(mut stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    stream.write_all(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
             Connection: keep-alive\r\n\r\nretry: {STREAM_RETRY_MS}\n\n"
        )
        .as_bytes(),
    )?;
    stream.flush()?;
    let (lock, signal) = &**shared;
    let mut seen = lock.lock().unwrap_or_else(|e| e.into_inner()).newest_ms();
    let ends = Instant::now() + STREAM_LIFETIME;
    while Instant::now() < ends {
        let event = {
            let history = lock.lock().unwrap_or_else(|e| e.into_inner());
            let (history, _) = signal
                .wait_timeout_while(history, KEEPALIVE, |history| {
                    history.newest_ms() == seen && history.last_error.is_none()
                })
                .unwrap_or_else(|e| e.into_inner());
            let newest = history.newest_ms();
            let error = history.last_error.clone();
            if newest != seen {
                seen = newest;
                Some(json!({ "t": newest, "error": error }).to_string())
            } else if error.is_some() {
                Some(json!({ "t": newest, "error": error }).to_string())
            } else {
                None
            }
        };
        match event {
            Some(data) => stream.write_all(format!("data: {data}\n\n").as_bytes())?,
            None => stream.write_all(b": keep-alive\n\n")?,
        }
        stream.flush()?;
        if lock.lock().unwrap_or_else(|e| e.into_inner()).last_error.is_some() {
            // While the service is unreachable the condition above holds at
            // once; pace the reports instead of spinning on them.
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    Ok(())
}

/// One `key=value` from a query string, percent-decoded.
pub(crate) fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (name == key).then(|| percent_decode(value))
    })
}

/// Decodes `%XX` escapes and `+` as a space; invalid escapes stay literal.
pub(crate) fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => out.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

