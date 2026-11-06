//! Purpose:
//! Prefork end-to-end tests for `elephc debug --web`: route election, a fiber
//! stopped inside one request, stepping, and disconnect-resumes-request.
//!
//! Called from:
//! - `tests/web_tests.rs` through `#[path = "web/debug.rs"] mod debug;`.
//!
//! Key details:
//! - The broker launches a real `--debug --web` server and speaks DAP over the
//!   broker's stdin/stdout; the HTTP fixture runs in a separate thread so the
//!   selected request can be stopped while another request completes.
//! - Every test uses its own 0700 session directory and `2` workers, and kills
//!   the prefork master from `server.pid` in `Drop` so a panic cannot leak it.
//! - The fixture's line numbers are part of the assertions: breakpoint at line 5
//!   (`$after = 20`) and a step to line 6.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{
    compile_web_with_flags, free_port, http_get_with_timeout, make_test_dir, wait_until_ready,
};

/// Source for the route/fiber test. Line 5 is the fiber breakpoint; `/pid`
/// lets the test pin one keep-alive connection to each worker.
const FIBER_FIXTURE: &str = "<?php\nfunction fiberWork(): void {\n    $inside = 10;\n    Fiber::suspend();\n    $after = 20;\n    echo \"fiber-done\\n\";\n}\nfunction selected(): void {\n    $fiber = new Fiber('fiberWork');\n    $fiber->start();\n    $consumer = 2;\n    $fiber->resume();\n    echo \"selected-done\\n\";\n}\n$path = $_SERVER['REQUEST_URI'] ?? '';\nif (str_starts_with($path, '/pid')) {\n    echo getmypid(), \"\\n\";\n} elseif (str_starts_with($path, '/selected')) {\n    selected();\n} else {\n    echo \"other-done\\n\";\n}\n";

/// Opens one keep-alive HTTP connection and identifies the worker behind it.
///
/// SO_REUSEPORT assigns a connection to one worker; sending the `/pid` probe and
/// keeping the socket open pins that worker for the next request. The test uses
/// two such connections with different PIDs, so a request stopped in one worker
/// cannot block the other.
fn open_worker_connection(address: &str) -> (TcpStream, u32) {
    let mut stream = TcpStream::connect(address).expect("connect worker probe");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("worker probe read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("worker probe write timeout");
    write!(
        stream,
        "GET /pid HTTP/1.1\r\nHost: {address}\r\nConnection: keep-alive\r\n\r\n"
    )
    .expect("write worker probe");
    let response = super::read_complete_http_response(&mut stream);
    let response = super::normalize_complete_http_response(response);
    let body = super::raw_response_body(&response).trim();
    let pid = body
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("worker probe did not return a pid: {response:?}"));
    (stream, pid)
}

/// Returns two keep-alive connections pinned to distinct workers.
fn pinned_worker_pair(address: &str) -> (TcpStream, TcpStream) {
    let mut connections = Vec::new();
    for _ in 0..8 {
        connections.push(open_worker_connection(address));
    }
    for left in 0..connections.len() {
        for right in (left + 1)..connections.len() {
            if connections[left].1 != connections[right].1 {
                let right = connections.swap_remove(right);
                let left = connections.swap_remove(left);
                return (left.0, right.0);
            }
        }
    }
    panic!("all worker probe connections landed on one worker");
}

/// Owns one broker process, its DAP frame reader and its session directory.
struct Broker {
    child: std::process::Child,
    stdin: Option<std::process::ChildStdin>,
    frames: Receiver<Value>,
    pending: VecDeque<Value>,
    seq: i64,
    stderr: Arc<Mutex<String>>,
    address: String,
    sessions: PathBuf,
}

impl Broker {
    /// Compiles no source; `binary` must already be a `--debug --web` artifact.
    fn spawn(dir: &Path, binary: &Path, route: &str) -> Self {
        Self::spawn_with(dir, binary, route, &["--workers", "2"])
    }

    /// Spawns the broker with explicit worker/handler-concurrency options.
    fn spawn_with(dir: &Path, binary: &Path, route: &str, extra: &[&str]) -> Self {
        let port = free_port();
        let address = format!("127.0.0.1:{port}");
        let sessions = dir.join("sessions");
        std::fs::create_dir_all(&sessions).expect("create web debug session directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&sessions, std::fs::Permissions::from_mode(0o700));
        }
        let binary = binary.to_string_lossy().into_owned();
        let mut command = std::process::Command::new(super::elephc_bin());
        command
            .current_dir(dir)
            .args(["debug", "--web", "--listen", &address, &binary, "--route", route])
            .args(extra)
            .arg("--web-dir")
            .arg(&sessions)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command.spawn().expect("spawn elephc debug --web");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("broker stdout");
        let stderr_pipe = child.stderr.take().expect("broker stderr");
        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_sink = stderr.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr_pipe);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                stderr_sink.lock().unwrap().push_str(&line);
                line.clear();
            }
        });
        let (tx, frames) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut content_length = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim_end_matches(['\r', '\n']);
                    if line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:") {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }
                let Some(length) = content_length else {
                    return;
                };
                let mut body = vec![0u8; length];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                let Ok(message) = serde_json::from_slice::<Value>(&body) else {
                    return;
                };
                if tx.send(message).is_err() {
                    return;
                }
            }
        });
        wait_until_ready(&address);
        Self {
            child,
            stdin,
            frames,
            pending: VecDeque::new(),
            seq: 0,
            stderr,
            address,
            sessions,
        }
    }

    /// Sends one framed DAP request and returns its `seq`.
    fn send(&mut self, command: &str, arguments: Value) -> i64 {
        self.seq += 1;
        let seq = self.seq;
        let message = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        let body = serde_json::to_vec(&message).expect("encode DAP request");
        let stdin = self.stdin.as_mut().expect("broker stdin");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write DAP header");
        stdin.write_all(&body).expect("write DAP body");
        stdin.flush().expect("flush DAP request");
        seq
    }

    /// Receives one frame, with a bounded wait and the broker log on failure.
    fn next_frame(&mut self) -> Value {
        match self.frames.recv_timeout(Duration::from_secs(30)) {
            Ok(message) => message,
            Err(error) => {
                let log = self.stderr.lock().unwrap().clone();
                panic!("DAP frame timeout ({error}); broker log:\n{log}");
            }
        }
    }

    /// Waits for one response, preserving unrelated frames for the other
    /// waiters. Responses can arrive interleaved with events and with a later
    /// request's response, so the pending queue is checked first.
    fn wait_response(&mut self, request_seq: i64) -> Value {
        loop {
            if let Some(index) = self.pending.iter().position(|message| {
                message["type"] == "response" && message["request_seq"] == request_seq
            }) {
                let message = self.pending.remove(index).expect("pending DAP response");
                assert_eq!(message["success"], true, "{message}");
                return message;
            }
            let message = self.next_frame();
            if message["type"] == "response" && message["request_seq"] == request_seq {
                assert_eq!(message["success"], true, "{message}");
                return message;
            }
            self.pending.push_back(message);
        }
    }

    /// Waits for one named event from the pending queue or the wire.
    fn wait_event(&mut self, event: &str) -> Value {
        loop {
            if let Some(index) = self.pending.iter().position(|message| {
                message["type"] == "event" && message["event"] == event
            }) {
                return self.pending.remove(index).expect("pending DAP event");
            }
            let message = self.next_frame();
            if message["type"] == "event" && message["event"] == event {
                return message;
            }
            self.pending.push_back(message);
        }
    }

    /// Closes the DAP stream and waits for the broker to detach.
    fn close(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    assert!(status.success(), "broker exited with {status}");
                    self.kill_server();
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                Ok(None) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    panic!("broker did not detach after DAP stdin closed");
                }
                Err(error) => panic!("cannot poll broker: {error}"),
            }
        }
    }

    /// Kills the prefork master written by the broker and waits for the port.
    fn kill_server(&self) {
        let pid_path = self.sessions.join("server.pid");
        let Ok(text) = std::fs::read_to_string(&pid_path) else {
            return;
        };
        let Ok(pid) = text.trim().parse::<i32>() else {
            return;
        };
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if std::net::TcpStream::connect(&self.address).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.kill_server();
    }
}

/// Verifies route election selects one prefork request, a fiber inside it has
/// its own DAP stack, and stepping follows the resumed fiber.
#[test]
fn web_debug_route_selects_one_request_and_steps_a_fiber() {
    let dir = make_test_dir("web_debug_route_fiber");
    let binary = compile_web_with_flags(&dir, FIBER_FIXTURE, "fiber", &["--debug"]);
    let source = dir.join("fiber.php");
    let mut broker = Broker::spawn(&dir, &binary, "/selected");

    // Queue the DAP handshake before the matching request exists: the broker
    // buffers stdin until a worker advertises, then forwards it to that worker.
    let initialize = broker.send(
        "initialize",
        json!({
            "clientID": "web-test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        }),
    );
    let launch = broker.send(
        "launch",
        json!({"program": source.to_string_lossy(), "stopOnEntry": true}),
    );
    let breakpoints = broker.send(
        "setBreakpoints",
        json!({
            "source": {"path": source.to_string_lossy()},
            "breakpoints": [{"line": 5}],
        }),
    );
    let configuration = broker.send("configurationDone", json!({}));

    let (selected_stream, other_stream) = pinned_worker_pair(&broker.address);
    let selected = thread::spawn(move || {
        let mut stream = selected_stream;
        write!(
            stream,
            "GET /selected HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
        )
        .expect("write selected request");
        let response = super::read_complete_http_response(&mut stream);
        super::normalize_complete_http_response(response)
    });

    broker.wait_response(initialize);
    broker.wait_event("initialized");
    broker.wait_response(breakpoints);
    broker.wait_response(launch);
    broker.wait_response(configuration);
    let entry = broker.wait_event("stopped");
    assert_eq!(entry["body"]["reason"], "entry", "{entry}");

    // The other route completes on its own pinned worker while the selected
    // request is stopped at entry. No second connection is opened here: a new
    // connection could be rerouted to the blocked worker by SO_REUSEPORT.
    let mut other_stream = other_stream;
    write!(
        other_stream,
        "GET /other HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .expect("write other request");
    let other = super::read_complete_http_response(&mut other_stream);
    let other = super::normalize_complete_http_response(other);
    assert!(
        super::raw_response_body(&other).contains("other-done"),
        "{other}"
    );
    eprintln!("web-debug: /other completed while /selected is stopped at entry");

    // The selected request resumes into the fiber and hits the line-5 breakpoint.
    let continue_entry = broker.send("continue", json!({"threadId": 1}));
    broker.wait_response(continue_entry);
    let hit = broker.wait_event("stopped");
    assert_eq!(hit["body"]["reason"], "breakpoint", "{hit}");
    assert!(
        hit["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("fiberWork")),
        "{hit}"
    );
    let fiber_thread = hit["body"]["threadId"].as_i64().expect("fiber thread");
    assert!(fiber_thread > 1, "fiber must have its own DAP thread: {hit}");

    let threads = broker.send("threads", json!({}));
    let threads = broker.wait_response(threads);
    assert!(
        threads["body"]["threads"]
            .as_array()
            .is_some_and(|threads| threads.iter().any(|thread| thread["id"] == fiber_thread)),
        "{threads}"
    );

    let stack = broker.send("stackTrace", json!({"threadId": fiber_thread}));
    let stack = broker.wait_response(stack);
    let names: Vec<&str> = stack["body"]["stackFrames"]
        .as_array()
        .expect("fiber stack frames")
        .iter()
        .filter_map(|frame| frame["name"].as_str())
        .collect();
    assert_eq!(names, ["fiberWork", "selected", "main"], "{stack}");
    eprintln!("web-debug: fiber thread {fiber_thread} stack {names:?}");

    let frame = stack["body"]["stackFrames"][0]["id"]
        .as_i64()
        .expect("fiber frame");
    let scopes = broker.send("scopes", json!({"frameId": frame}));
    let scopes = broker.wait_response(scopes);
    let variables_reference = scopes["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("locals reference");
    let variables = broker.send(
        "variables",
        json!({"variablesReference": variables_reference}),
    );
    let variables = broker.wait_response(variables);
    assert!(
        variables["body"]["variables"]
            .as_array()
            .is_some_and(|variables| variables.iter().any(|variable| variable["name"] == "$inside")),
        "{variables}"
    );

    let next = broker.send("next", json!({"threadId": fiber_thread}));
    broker.wait_response(next);
    let stepped = broker.wait_event("stopped");
    assert_eq!(stepped["body"]["reason"], "step", "{stepped}");
    assert!(
        stepped["body"]["description"]
            .as_str()
            .is_some_and(|description| description.contains(":6:")),
        "{stepped}"
    );
    eprintln!(
        "web-debug: stepped to {:?}",
        stepped["body"]["description"]
    );

    let stack = broker.send("stackTrace", json!({"threadId": fiber_thread}));
    let stack = broker.wait_response(stack);
    let frame = stack["body"]["stackFrames"][0]["id"]
        .as_i64()
        .expect("stepped frame");
    let scopes = broker.send("scopes", json!({"frameId": frame}));
    let scopes = broker.wait_response(scopes);
    let variables_reference = scopes["body"]["scopes"][0]["variablesReference"]
        .as_i64()
        .expect("stepped locals reference");
    let variables = broker.send(
        "variables",
        json!({"variablesReference": variables_reference}),
    );
    let variables = broker.wait_response(variables);
    assert!(
        variables["body"]["variables"]
            .as_array()
            .is_some_and(|variables| variables.iter().any(|variable| variable["name"] == "$after"
                && variable["value"] == "int(20)")),
        "{variables}"
    );
    eprintln!("web-debug: $after = int(20) after the step");

    let resume = broker.send("continue", json!({"threadId": fiber_thread}));
    broker.wait_response(resume);
    let selected = selected.join().expect("join selected request");
    assert!(selected.contains("fiber-done"), "{selected}");
    assert!(selected.contains("selected-done"), "{selected}");
    eprintln!(
        "web-debug: selected response {:?}",
        super::raw_response_body(&selected).trim()
    );
    broker.close();
}

/// Verifies a DAP disconnect resumes the selected request and leaves the
/// prefork master serving other routes.
#[test]
fn web_debug_disconnect_resumes_request_and_keeps_server_healthy() {
    let dir = make_test_dir("web_debug_disconnect");
    let binary = compile_web_with_flags(
        &dir,
        "<?php\n$path = $_SERVER['REQUEST_URI'] ?? '';\nif (str_starts_with($path, '/selected')) {\n    echo \"selected-done\\n\";\n} else {\n    echo \"other-done\\n\";\n}\n",
        "disconnect",
        &["--debug"],
    );
    let source = dir.join("disconnect.php");
    let mut broker = Broker::spawn(&dir, &binary, "/selected");

    let initialize = broker.send(
        "initialize",
        json!({
            "clientID": "web-test",
            "adapterID": "elephc",
            "linesStartAt1": true,
            "columnsStartAt1": true,
        }),
    );
    let launch = broker.send(
        "launch",
        json!({"program": source.to_string_lossy(), "stopOnEntry": true}),
    );
    let configuration = broker.send("configurationDone", json!({}));

    let address = broker.address.clone();
    let selected = thread::spawn(move || {
        http_get_with_timeout(&address, "/selected", Duration::from_secs(20))
            .expect("selected request")
    });
    broker.wait_response(initialize);
    broker.wait_event("initialized");
    broker.wait_response(launch);
    broker.wait_response(configuration);
    let entry = broker.wait_event("stopped");
    assert_eq!(entry["body"]["reason"], "entry", "{entry}");

    let disconnect = broker.send("disconnect", json!({}));
    broker.wait_response(disconnect);
    let selected = selected.join().expect("join selected request");
    assert!(selected.contains("selected-done"), "{selected}");

    let other = http_get_with_timeout(&broker.address, "/other", Duration::from_secs(5))
        .expect("other request after disconnect");
    assert!(other.contains("other-done"), "{other}");
    eprintln!(
        "web-debug: disconnect resumed {:?}; server still answered /other",
        selected.trim()
    );
    broker.close();
}

/// Verifies request- and pool-isolated handlers invoke the route hook in the
/// handler child: the selected request stops (and steps a fiber) while a second
/// handler child completes `/other`, with one async worker and the master alive.
#[test]
fn web_debug_request_and_pool_isolation_route_and_fiber() {
    for mode in ["request", "pool"] {
        let dir = make_test_dir(&format!("web_debug_isolated_{mode}"));
        let isolation = format!("--web-isolation={mode}");
        let binary = compile_web_with_flags(
            &dir,
            FIBER_FIXTURE,
            mode,
            &["--debug", isolation.as_str()],
        );
        let source = dir.join(format!("{mode}.php"));
        let mut broker = Broker::spawn_with(
            &dir,
            &binary,
            "/selected",
            &["--workers", "1", "--handler-concurrency", "2"],
        );

        let initialize = broker.send(
            "initialize",
            json!({
                "clientID": "web-test",
                "adapterID": "elephc",
                "linesStartAt1": true,
                "columnsStartAt1": true,
            }),
        );
        let launch = broker.send(
            "launch",
            json!({"program": source.to_string_lossy(), "stopOnEntry": true}),
        );
        let breakpoints = broker.send(
            "setBreakpoints",
            json!({
                "source": {"path": source.to_string_lossy()},
                "breakpoints": [{"line": 5}],
            }),
        );
        let configuration = broker.send("configurationDone", json!({}));

        let address = broker.address.clone();
        let selected = thread::spawn(move || {
            http_get_with_timeout(&address, "/selected", Duration::from_secs(20))
                .expect("selected isolated request")
        });
        broker.wait_response(initialize);
        broker.wait_event("initialized");
        broker.wait_response(launch);
        broker.wait_response(breakpoints);
        broker.wait_response(configuration);
        let entry = broker.wait_event("stopped");
        assert_eq!(entry["body"]["reason"], "entry", "{mode}: {entry}");

        // One worker, one async connection task per request. The selected
        // handler child is stopped; a second handler child must serve /other.
        let other = http_get_with_timeout(&broker.address, "/other", Duration::from_secs(5))
            .expect("other isolated request while selected is stopped");
        assert!(other.contains("other-done"), "{mode}: {other}");
        eprintln!("web-debug {mode}: /other completed while /selected is stopped");

        let continue_entry = broker.send("continue", json!({"threadId": 1}));
        broker.wait_response(continue_entry);
        let hit = broker.wait_event("stopped");
        assert_eq!(hit["body"]["reason"], "breakpoint", "{mode}: {hit}");
        let fiber_thread = hit["body"]["threadId"].as_i64().expect("fiber thread");
        assert!(fiber_thread > 1, "{mode}: {hit}");
        let stack = broker.send("stackTrace", json!({"threadId": fiber_thread}));
        let stack = broker.wait_response(stack);
        let names: Vec<&str> = stack["body"]["stackFrames"]
            .as_array()
            .expect("isolated fiber stack")
            .iter()
            .filter_map(|frame| frame["name"].as_str())
            .collect();
        assert_eq!(
            names,
            ["fiberWork", "selected", "main"],
            "{mode}: {stack}"
        );
        eprintln!("web-debug {mode}: fiber thread {fiber_thread} stack {names:?}");

        let next = broker.send("next", json!({"threadId": fiber_thread}));
        broker.wait_response(next);
        let stepped = broker.wait_event("stopped");
        assert_eq!(stepped["body"]["reason"], "step", "{mode}: {stepped}");
        assert!(
            stepped["body"]["description"]
                .as_str()
                .is_some_and(|description| description.contains(":6:")),
            "{mode}: {stepped}"
        );

        let resume = broker.send("continue", json!({"threadId": fiber_thread}));
        broker.wait_response(resume);
        let selected = selected.join().expect("join isolated selected request");
        assert!(
            selected.contains("fiber-done") && selected.contains("selected-done"),
            "{mode}: {selected}"
        );
        eprintln!(
            "web-debug {mode}: selected response {:?}",
            super::raw_response_body(&selected).trim()
        );

        let healthy = http_get_with_timeout(&broker.address, "/other", Duration::from_secs(5))
            .expect("isolated server after the session");
        assert!(healthy.contains("other-done"), "{mode}: {healthy}");
        eprintln!("web-debug {mode}: server still healthy after the session");
        broker.close();
    }
}
