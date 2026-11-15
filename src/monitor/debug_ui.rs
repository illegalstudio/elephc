//! Purpose:
//! Serves the interactive browser debugger for `elephc monitor --debug-ui`.
//! A launched debug build talks DAP over an inherited socket; this module
//! bridges that stream to a tiny HTTP API and the self-contained page.
//!
//! Called from:
//! - `crate::monitor::run` when `--debug-ui` is selected.
//!
//! Key details:
//! - The browser never sees a raw socket: it posts DAP requests and long-polls
//!   events, so the page works through ordinary HTTP proxies.
//! - The bridge is loopback-only by default; binding another address exposes
//!   full debug control and is the operator's explicit choice.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::io::IntoRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::MonitorCommand;

/// Self-contained debugger page.
const DEBUG_UI_HTML: &str = include_str!("debug_ui.html");
/// Events retained for a page that reconnects or polls slowly.
const MAX_EVENTS: usize = 8192;
/// Long-poll wait before returning an empty batch.
const POLL_TIMEOUT: Duration = Duration::from_secs(20);
/// Placeholder replaced with the per-run session token in the served page.
const TOKEN_PLACEHOLDER: &str = "__ELEPHC_DEBUG_TOKEN__";
/// Largest accepted HTTP request body; DAP frames are tiny.
const MAX_REQUEST_BODY: usize = 8 * 1024 * 1024;

/// Runs one `monitor --debug-ui` session.
pub(crate) fn run(cmd: &MonitorCommand) -> i32 {
    let binary = match debug_binary(&cmd.target, cmd.allow_eval) {
        Ok(binary) => binary,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    let (adapter, child) = match UnixStream::pair() {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("elephc monitor: cannot create the DAP socket: {error}");
            return 1;
        }
    };
    let child_fd = child.into_raw_fd();
    let mut process = Command::new(&binary);
    process
        .env("ELEPHC_DEBUG_DAP", "1")
        .env("ELEPHC_DEBUG_FD", "3")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    unsafe {
        process.pre_exec(move || {
            if libc::dup2(2, 1) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::dup2(child_fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            unsafe {
                libc::close(child_fd);
            }
            eprintln!(
                "elephc monitor: failed to run '{}': {error}",
                binary.display()
            );
            return 1;
        }
    };
    unsafe {
        libc::close(child_fd);
    }
    let bridge = DebugBridge::start(adapter, Some(child.id()));
    let requested = cmd.serve.as_deref().unwrap_or("127.0.0.1:0");
    let listener = match std::net::TcpListener::bind(requested) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("elephc monitor: cannot serve debugger on {requested}: {error}");
            let _ = child.kill();
            let _ = child.wait();
            return 1;
        }
    };
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("elephc monitor: cannot read debugger address: {error}");
            let _ = child.kill();
            let _ = child.wait();
            return 1;
        }
    };
    eprintln!("elephc monitor: debugger at http://{address}/");
    let token = session_token();
    let page = Arc::new(DEBUG_UI_HTML.replace(TOKEN_PLACEHOLDER, &token));
    let bridge_for_server = Arc::clone(&bridge);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let bridge = Arc::clone(&bridge_for_server);
            let page = Arc::clone(&page);
            let token = token.clone();
            let bound = address;
            std::thread::spawn(move || {
                let _ = serve_http(stream, &bridge, &page, &token, bound);
            });
        }
    });
    let status = child.wait();
    // Keep the page's final long-poll alive long enough to deliver the last
    // output and terminated events before this process exits.
    std::thread::sleep(Duration::from_secs(2));
    match status {
        Ok(status) if status.success() => 0,
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("elephc monitor: cannot wait for debuggee: {error}");
            1
        }
    }
}

/// Compiles a `.php` source with `--debug`, or returns an existing binary path.
fn debug_binary(target: &str, allow_eval: bool) -> Result<PathBuf, String> {
    let source = Path::new(target);
    if source.extension().and_then(|extension| extension.to_str()) != Some("php") {
        let binary = super::spawnable_path(target);
        if !binary.exists() {
            return Err(format!("'{}' does not exist", binary.display()));
        }
        return Ok(binary);
    }
    let compiler = std::env::current_exe()
        .map_err(|error| format!("cannot locate the compiler: {error}"))?;
    let mut command = Command::new(compiler);
    command.arg("--debug");
    if allow_eval {
        command.arg("--allow-eval");
    }
    let status = command
        .arg(target)
        .status()
        .map_err(|error| format!("cannot compile '{target}': {error}"))?;
    if !status.success() {
        return Err(format!("compiling '{target}' with --debug failed"));
    }
    Ok(super::spawnable_path(target.trim_end_matches(".php")))
}

/// One retained DAP message batch shared by the reader and HTTP handlers.
struct EventLog {
    /// Absolute index of `messages[0]`.
    base: u64,
    /// Recent messages, oldest first.
    messages: VecDeque<serde_json::Value>,
    /// Whether the debuggee's DAP stream closed.
    closed: bool,
}

/// Shared DAP writer plus the event log used by the browser bridge.
struct DebugBridge {
    writer: Mutex<UnixStream>,
    log: Mutex<EventLog>,
    ready: Condvar,
    /// Debuggee pid, used to forward a browser `pause` request as SIGUSR1.
    pid: Option<u32>,
}

impl DebugBridge {
    /// Starts the reader thread and returns the bridge shared by HTTP handlers.
    fn start(adapter: UnixStream, pid: Option<u32>) -> Arc<Self> {
        let writer = adapter
            .try_clone()
            .expect("clone the debuggee DAP writer");
        let bridge = Arc::new(Self {
            writer: Mutex::new(writer),
            log: Mutex::new(EventLog {
                base: 0,
                messages: VecDeque::new(),
                closed: false,
            }),
            ready: Condvar::new(),
            pid,
        });
        let reader_bridge = Arc::clone(&bridge);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(adapter);
            loop {
                let Some(message) = read_dap_message(&mut reader) else {
                    break;
                };
                if let Ok(mut log) = reader_bridge.log.lock() {
                    if log.messages.len() >= MAX_EVENTS {
                        log.messages.pop_front();
                        log.base += 1;
                    }
                    log.messages.push_back(message);
                    reader_bridge.ready.notify_all();
                }
            }
            if let Ok(mut log) = reader_bridge.log.lock() {
                log.closed = true;
                reader_bridge.ready.notify_all();
            }
        });
        bridge
    }

    /// Writes one already-JSON-encoded DAP request to the debuggee.
    fn send(&self, body: &[u8]) -> std::io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "DAP writer poisoned"))?;
        write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
        writer.write_all(body)?;
        writer.flush()?;
        // The runtime reads DAP only while stopped; a `pause` request must also
        // interrupt a running program, exactly like the CLI adapter does.
        if is_pause_request(body) {
            if let Some(pid) = self.pid {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGUSR1);
                }
            }
        }
        Ok(())
    }

    /// Waits for messages newer than `since`, returning the batch and next index.
    fn events_since(&self, since: u64) -> (Vec<serde_json::Value>, u64, bool) {
        let deadline = Instant::now() + POLL_TIMEOUT;
        let Ok(mut log) = self.log.lock() else {
            return (Vec::new(), since, true);
        };
        loop {
            let next = log.base + log.messages.len() as u64;
            if next > since || log.closed {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (guard, _) = self
                .ready
                .wait_timeout(log, deadline.saturating_duration_since(now))
                .unwrap_or_else(|error| error.into_inner());
            log = guard;
        }
        let start = since.saturating_sub(log.base).min(log.messages.len() as u64) as usize;
        let events = log.messages.iter().skip(start).cloned().collect();
        let next = log.base + log.messages.len() as u64;
        let closed = log.closed;
        (events, next, closed)
    }
}

/// Reads one `Content-Length` framed DAP message.
fn read_dap_message(reader: &mut BufReader<UnixStream>) -> Option<serde_json::Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; length?];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Handles one HTTP request from the debugger page.
fn serve_http(
    mut stream: std::net::TcpStream,
    bridge: &DebugBridge,
    page: &str,
    token: &str,
    bound: std::net::SocketAddr,
) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or("/");
    let mut content_length = 0usize;
    let mut host = String::new();
    let mut origin = None;
    let mut request_token = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "content-length" => {
                if let Ok(value) = value.parse::<usize>() {
                    content_length = value;
                }
            }
            "host" => host = value.to_string(),
            "origin" => origin = Some(value.to_string()),
            "x-elephc-debug-token" => request_token = value.to_string(),
            _ => {}
        }
    }
    if content_length > MAX_REQUEST_BODY {
        return serve_body(
            &mut stream,
            "413 Payload Too Large",
            "application/json",
            br#"{"error":"request body too large"}"#,
        );
    }
    // The page embeds a fresh per-run token and every command carries it. A
    // hostile cross-site `text/plain` POST can neither read the page nor guess
    // the token, and a foreign Origin is rejected even before the token check.
    if !origin_allowed(origin.as_deref(), &host) {
        drain_request_body(&mut reader, content_length);
        return serve_body(
            &mut stream,
            "403 Forbidden",
            "application/json",
            br#"{"error":"forbidden origin"}"#,
        );
    }
    if !host_allowed(&host, bound) {
        drain_request_body(&mut reader, content_length);
        return serve_body(
            &mut stream,
            "403 Forbidden",
            "application/json",
            br#"{"error":"forbidden host"}"#,
        );
    }
    let state_changing = path == "/dap/request" || path == "/dap/events";
    if state_changing && request_token != token {
        drain_request_body(&mut reader, content_length);
        return serve_body(
            &mut stream,
            "403 Forbidden",
            "application/json",
            br#"{"error":"missing or invalid debugger token"}"#,
        );
    }
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => serve_body(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            page.as_bytes(),
        ),
        ("GET", "/dap/events") => {
            let since = target
                .split_once("since=")
                .and_then(|(_, value)| value.split('&').next())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            let (events, next, closed) = bridge.events_since(since);
            let body = serde_json::json!({
                "events": events,
                "next": next,
                "closed": closed,
            });
            serve_body(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.to_string().as_bytes(),
            )
        }
        ("POST", "/dap/request") => {
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;
            if serde_json::from_slice::<serde_json::Value>(&body).is_err() {
                return serve_body(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    br#"{"error":"invalid JSON"}"#,
                );
            }
            match bridge.send(&body) {
                Ok(()) => serve_body(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    br#"{"accepted":true}"#,
                ),
                Err(error) => {
                    let body = serde_json::json!({"error": error.to_string()}).to_string();
                    serve_body(
                        &mut stream,
                        "502 Bad Gateway",
                        "application/json",
                        body.as_bytes(),
                    )
                }
            }
        }
        _ => serve_body(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found\n",
        ),
    }
}

/// Consumes a rejected request body so closing the socket sends the response
/// instead of an RST caused by unread bytes.
fn drain_request_body(reader: &mut BufReader<std::net::TcpStream>, content_length: usize) {
    if content_length == 0 {
        return;
    }
    let mut body = vec![0u8; content_length];
    let _ = reader.read_exact(&mut body);
}

/// Returns whether a request's `Origin`, when present, is the server itself.
///
/// Browsers send `Origin` on cross-origin fetches and on same-origin POSTs. The
/// authority must equal the `Host` header, so `https://evil.example` cannot
/// drive a loopback debugger even with a `text/plain` POST.
fn origin_allowed(origin: Option<&str>, host: &str) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    matches!(scheme, "http" | "https")
        && !host.is_empty()
        && authority.eq_ignore_ascii_case(host)
}

/// Returns whether a request `Host` names the loopback listener.
///
/// A DNS-rebinding page reaches 127.0.0.1 while keeping its own hostname in the
/// `Host` header; requiring a loopback literal (or `localhost`) blocks it. A
/// non-loopback bind is an explicit operator choice and accepts its own hosts.
fn host_allowed(host: &str, bound: std::net::SocketAddr) -> bool {
    if !bound.ip().is_loopback() {
        return !host.is_empty();
    }
    let name = host
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(name, _)| name))
        .unwrap_or_else(|| host.split(':').next().unwrap_or(host));
    name.eq_ignore_ascii_case("localhost")
        || name == bound.ip().to_string()
        || name == "127.0.0.1"
        || name == "::1"
}

/// Reads one unpredictable per-run session token from `/dev/urandom`.
fn session_token() -> String {
    use std::io::Read as _;
    let mut bytes = [0u8; 32];
    let random = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes));
    if random.is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let seed = format!("{}-{nanos}", std::process::id());
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = seed.as_bytes()[index % seed.len()];
        }
    }
    let mut token = String::with_capacity(64);
    for byte in bytes {
        let _ = std::fmt::Write::write_fmt(&mut token, format_args!("{byte:02x}"));
    }
    token
}

/// Returns whether one DAP request body asks the debuggee to pause.
fn is_pause_request(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|message| message.get("command").and_then(serde_json::Value::as_str).map(str::to_string))
        .is_some_and(|command| command == "pause")
}

/// Writes one complete HTTP response.
fn serve_body(
    stream: &mut std::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}
