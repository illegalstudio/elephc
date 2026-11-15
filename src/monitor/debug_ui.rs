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
    let bridge = DebugBridge::start(adapter);
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
    let bridge_for_server = Arc::clone(&bridge);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let bridge = Arc::clone(&bridge_for_server);
            std::thread::spawn(move || {
                let _ = serve_http(stream, &bridge);
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
}

impl DebugBridge {
    /// Starts the reader thread and returns the bridge shared by HTTP handlers.
    fn start(adapter: UnixStream) -> Arc<Self> {
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
        writer.flush()
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
fn serve_http(mut stream: std::net::TcpStream, bridge: &DebugBridge) -> std::io::Result<()> {
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
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
        {
            content_length = value;
        }
    }
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => serve_body(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            DEBUG_UI_HTML.as_bytes(),
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
