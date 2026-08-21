//! Purpose:
//! End-to-end tests for `--web`: compile PHP into a prefork HTTP server binary,
//! launch it with `--listen`, drive it over raw TCP, and assert the response.
//!
//! Called from:
//! - `cargo test --test web_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir with an isolated runtime cache, mirroring cdylib_tests.
//! - The HTTP client is a hand-written minimal HTTP/1.1 request over a
//!   std::net::TcpStream so the test pulls in no HTTP client dependency.
//! - Host-target only: each platform/arch covers itself (macOS aarch64 local,
//!   Linux x86_64/aarch64 via the Docker test scripts).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` in web mode with extra compiler flags and returns the binary path.
fn compile_web_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web").args(flags).arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --web failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` in web mode without extra compiler flags.
fn compile_web(dir: &Path, source: &str, stem: &str) -> PathBuf {
    compile_web_with_flags(dir, source, stem, &[])
}

/// Compiles one explicit web-isolation model for broker-specific tests.
fn compile_isolated_web(dir: &Path, source: &str, stem: &str, mode: &str) -> PathBuf {
    let flag = match mode {
        "pool" => "--web-isolation=pool",
        "request" => "--web-isolation=request",
        other => panic!("unsupported isolated web test mode: {other}"),
    };
    compile_web_with_flags(dir, source, stem, &[flag])
}

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// Blocks until `addr` accepts a TCP connection (server ready), or panics after 10s.
fn wait_until_ready(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {}", addr);
}

/// RAII guard around a spawned web-server child process.
///
/// `std::process::Child` does *not* kill the process when its handle is
/// dropped, so any test that panics before its manual `child.kill()` — a failed
/// assertion, or an `unwrap` in `http_get`/`http_request`/`wait_until_ready` —
/// leaks a resident server (plus its prefork workers, which stay alive while the
/// master does). Under load that accumulation exhausts memory and triggers the
/// OS OOM killer. Wrapping the child in this guard makes `Drop` reap it
/// unconditionally, even while unwinding, so a failing test can never leak a
/// server. Killing the master reaps its workers (verified: they exit on parent
/// death), so the guard only needs to kill the master.
struct ServerGuard {
    child: std::process::Child,
}

impl ServerGuard {
    /// Wraps an already-spawned server child so it is reaped on scope exit.
    fn new(child: std::process::Child) -> Self {
        Self { child }
    }

    /// Terminates the server gracefully so its prefork workers are reaped too.
    ///
    /// `std::process::Child::kill` sends `SIGKILL`, which the master cannot
    /// trap — so its worker children are reparented to `launchd`/`init` and
    /// survive as orphans (verified: `SIGKILL` on the master leaves the workers
    /// running). Across a suite that spawns dozens of servers those orphans
    /// accumulate and exhaust memory. Sending `SIGTERM` first lets the master
    /// run its shutdown path and reap its own workers; a `SIGKILL` fallback
    /// covers a wedged master after a short grace period. This inherent method
    /// shadows `Child::kill` through the `Deref`, so every existing
    /// `child.kill()` call site becomes graceful with no change. Idempotent: an
    /// already-exited child returns `Ok` immediately.
    fn kill(&mut self) -> std::io::Result<()> {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return Ok(());
        }
        let pid = self.child.id().to_string();
        let _ = Command::new("kill").arg("-TERM").arg(&pid).status();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Wedged master: force-kill. Its workers may briefly orphan, but this
        // path is the rare exception, not the steady-state teardown.
        self.child.kill()
    }
}

impl std::ops::Deref for ServerGuard {
    type Target = std::process::Child;
    /// Exposes the wrapped child for read-only access (`id`, `stdout`).
    fn deref(&self) -> &std::process::Child {
        &self.child
    }
}

impl std::ops::DerefMut for ServerGuard {
    /// Exposes the wrapped child for `kill`/`wait`/`try_wait`/`stdout.take()`.
    fn deref_mut(&mut self) -> &mut std::process::Child {
        &mut self.child
    }
}

impl Drop for ServerGuard {
    /// Gracefully terminates and reaps the server unconditionally, even during a
    /// panic unwind, so neither the master nor its workers leak. Best-effort: an
    /// already-reaped child is a no-op.
    fn drop(&mut self) {
        let _ = self.kill();
        let _ = self.child.wait();
    }
}

/// Spawns the server binary on `addr`, waits until it accepts connections, and
/// returns an RAII [`ServerGuard`] that reaps it on scope exit. The child is
/// wrapped in the guard *before* `wait_until_ready`, so a readiness-timeout
/// panic still reaps the process instead of orphaning it.
fn spawn_server(bin: &Path, addr: &str, workers: &str) -> ServerGuard {
    spawn_server_with_args(bin, addr, workers, &[])
}

/// Spawns a server with additional runtime options and waits for readiness.
fn spawn_server_with_args(
    bin: &Path,
    addr: &str,
    workers: &str,
    extra_args: &[&str],
) -> ServerGuard {
    let child = ServerGuard::new(
        Command::new(bin)
            .arg("--listen").arg(addr)
            .arg("--workers").arg(workers)
            .args(extra_args)
            .spawn()
            .expect("failed to spawn web server"),
    );
    wait_until_ready(addr);
    child
}

/// Sends one HTTP/1.1 GET and returns the response with any complete chunked body decoded.
fn http_get(addr: &str, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    normalize_complete_http_response(buf)
}

/// Decodes a complete chunked response body for assertions while preserving its headers.
///
/// Intentionally incomplete responses are returned unchanged so crash/timeout tests can still
/// inspect the exact transfer framing emitted before the connection was aborted.
fn normalize_complete_http_response(response: String) -> String {
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return response;
    };
    let is_chunked = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    });
    if !is_chunked {
        return response;
    }
    let Some(decoded) = decode_complete_chunked_body(body.as_bytes()) else {
        return response;
    };
    format!("{headers}\r\n\r\n{}", String::from_utf8_lossy(&decoded))
}

/// Decodes one complete HTTP chunk stream and rejects truncated or malformed framing.
fn decode_complete_chunked_body(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let size_end = body.windows(2).position(|window| window == b"\r\n")?;
        let size_line = std::str::from_utf8(&body[..size_end]).ok()?;
        let size_text = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_text, 16).ok()?;
        body = &body[size_end + 2..];
        if size == 0 {
            return body.starts_with(b"\r\n").then_some(decoded);
        }
        let chunk = body.get(..size)?;
        body = body.get(size..)?;
        if !body.starts_with(b"\r\n") {
            return None;
        }
        decoded.extend_from_slice(chunk);
        body = &body[2..];
    }
}

/// Reads exactly one complete HTTP response without waiting for a keep-alive socket to close.
fn read_complete_http_response(stream: &mut TcpStream) -> String {
    let mut response = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).expect("read HTTP response");
        assert!(read > 0, "connection closed before a complete HTTP response");
        response.extend_from_slice(&chunk[..read]);
        assert!(response.len() <= 1024 * 1024, "HTTP test response exceeded 1 MiB");

        let text = String::from_utf8_lossy(&response);
        let Some((headers, body)) = text.split_once("\r\n\r\n") else {
            continue;
        };
        let is_chunked = headers.lines().any(|line| {
            let Some((name, value)) = line.split_once(':') else {
                return false;
            };
            name.eq_ignore_ascii_case("transfer-encoding")
                && value
                    .split(',')
                    .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
        });
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        });
        if (is_chunked && decode_complete_chunked_body(body.as_bytes()).is_some())
            || content_length.is_some_and(|length| body.len() >= length)
            || (!is_chunked && content_length.is_none())
        {
            return normalize_complete_http_response(text.into_owned());
        }
    }
}

/// Verifies the web test client decodes complete chunks but preserves an interrupted stream.
#[test]
fn web_test_client_normalizes_only_complete_chunked_responses() {
    let complete = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n";
    assert_eq!(
        normalize_complete_http_response(complete.to_string()),
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nok"
    );

    let interrupted = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n";
    assert_eq!(
        normalize_complete_http_response(interrupted.to_string()),
        interrupted
    );
}

/// Returns direct child PIDs from procfs on supported Linux targets.
#[cfg(target_os = "linux")]
fn direct_child_process_ids(parent_pid: u32) -> Vec<u32> {
    let path = format!("/proc/{parent_pid}/task/{parent_pid}/children");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"))
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
        .collect()
}

/// Returns direct child PIDs from the BSD process table on macOS.
#[cfg(target_os = "macos")]
fn direct_child_process_ids(parent_pid: u32) -> Vec<u32> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .expect("failed to inspect the web process tree");
    assert!(output.status.success(), "ps failed while inspecting web children");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let pid = columns.next()?.parse::<u32>().ok()?;
            let ppid = columns.next()?.parse::<u32>().ok()?;
            (ppid == parent_pid).then_some(pid)
        })
        .collect()
}

/// Waits until a process has exactly `expected` direct children, returning their PIDs.
fn wait_for_direct_child_count(parent_pid: u32, expected: usize) -> Vec<u32> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let children = direct_child_process_ids(parent_pid);
        if children.len() == expected {
            return children;
        }
        assert!(
            Instant::now() < deadline,
            "PID {parent_pid} retained {} direct children; expected {expected}: {children:?}",
            children.len()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Returns whether a PID still names a live or zombie process.
fn process_exists(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Waits for every recorded descendant PID to disappear after master shutdown.
fn wait_for_processes_to_exit(pids: &[u32]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let live = pids
            .iter()
            .copied()
            .filter(|pid| process_exists(*pid))
            .collect::<Vec<_>>();
        if live.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "web shutdown left descendant PIDs alive: {live:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Sends a bounded HTTP request so a dead broker cannot hang the test harness.
fn http_get_with_timeout(addr: &str, path: &str, timeout: Duration) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(normalize_complete_http_response(response))
}

/// Returns the raw HTTP body section while leaving transfer framing intact.
fn raw_response_body(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

/// Reads all bytes delivered before EOF, reset, or timeout so tests can inspect
/// intentionally aborted HTTP responses without treating the abort as a harness failure.
fn read_raw_response_allowing_abort(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                break;
            }
            Err(error) => panic!("failed to read raw HTTP response: {error}"),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Verifies a trivial program compiles under --web and produces an executable file.
#[test]
fn web_compile_produces_binary() {
    let dir = make_test_dir("web_compile");
    let bin = compile_web(&dir, "<?php echo \"Hello World\";", "app");
    assert!(bin.exists(), "expected binary at {}", bin.display());
}

/// Verifies each compile-time model produces its intended idle process topology
/// and removes every recorded worker, broker, and handler PID during shutdown.
#[test]
fn web_isolation_modes_have_expected_process_trees() {
    for (mode, expected_handler_children) in [
        ("worker", 0usize),
        ("pool", 2usize),
        ("request", 0usize),
    ] {
        let dir = make_test_dir(&format!("web_process_tree_{mode}"));
        let bin = if mode == "worker" {
            compile_web(&dir, "<?php echo 'ok';", "app")
        } else {
            compile_isolated_web(&dir, "<?php echo 'ok';", "app", mode)
        };
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let extra = if mode == "worker" {
            Vec::new()
        } else {
            vec!["--handler-concurrency", "2"]
        };
        let mut server = spawn_server_with_args(&bin, &addr, "1", &extra);
        let worker = wait_for_direct_child_count(server.id(), 1)[0];
        let mut descendants = vec![worker];
        if mode == "worker" {
            assert!(
                direct_child_process_ids(worker).is_empty(),
                "default worker isolation unexpectedly started a broker"
            );
        } else {
            let broker = wait_for_direct_child_count(worker, 1)[0];
            descendants.push(broker);
            descendants.extend(wait_for_direct_child_count(
                broker,
                expected_handler_children,
            ));
        }
        let response = http_get(&addr, "/");
        assert!(
            raw_response_body(&response).contains("ok"),
            "{mode} isolation failed its smoke request: {response:?}"
        );
        let _ = server.kill();
        let _ = server.wait();
        wait_for_processes_to_exit(&descendants);
    }
}

/// Verifies pool children persist across requests and retire at their exact quota.
#[test]
fn web_pool_reuses_and_recycles_handler_children() {
    let dir = make_test_dir("web_pool_recycle");
    let bin = compile_isolated_web(&dir, "<?php echo 'ok';", "app", "pool");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server_with_args(
        &bin,
        &addr,
        "1",
        &[
            "--handler-concurrency",
            "1",
            "--max-handler-requests",
            "2",
        ],
    );
    let worker = wait_for_direct_child_count(server.id(), 1)[0];
    let broker = wait_for_direct_child_count(worker, 1)[0];
    let initial = wait_for_direct_child_count(broker, 1)[0];

    assert!(raw_response_body(&http_get(&addr, "/")).contains("ok"));
    assert_eq!(
        wait_for_direct_child_count(broker, 1)[0],
        initial,
        "pool child was replaced before reaching its request quota"
    );
    assert!(raw_response_body(&http_get(&addr, "/")).contains("ok"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let replacement = loop {
        let children = direct_child_process_ids(broker);
        if let Some(pid) = children.first().copied().filter(|pid| *pid != initial) {
            break pid;
        }
        assert!(Instant::now() < deadline, "pool child did not retire at its quota");
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_ne!(replacement, initial);

    let _ = server.kill();
    let _ = server.wait();
}

/// Regression: concurrent pool traffic must tolerate handlers that close immediately
/// after a framed response, while still keeping SIGPIPE contained as an I/O error.
#[test]
fn web_pool_worker_survives_concurrent_connection_load() {
    let dir = make_test_dir("web_pool_sigpipe");
    let bin = compile_isolated_web(&dir, "<?php echo 'ok';", "app", "pool");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server_with_args(
        &bin,
        &addr,
        "1",
        &[
            "--handler-concurrency",
            "8",
            "--max-handler-requests",
            "0",
        ],
    );
    let worker = wait_for_direct_child_count(server.id(), 1)[0];
    let clients = (0..8)
        .map(|_| {
            let addr = addr.clone();
            std::thread::spawn(move || {
                for request in 0..100 {
                    let response = http_get(&addr, "/");
                    assert!(
                        response.starts_with("HTTP/1.1 200")
                            && raw_response_body(&response).contains("ok"),
                        "concurrent pool request {request} failed: {response:?}"
                    );
                }
            })
        })
        .collect::<Vec<_>>();
    for client in clients {
        client.join().expect("concurrent pool client panicked");
    }

    assert_eq!(
        wait_for_direct_child_count(server.id(), 1),
        vec![worker],
        "concurrent pool load unexpectedly recycled the web worker"
    );
    let _ = server.kill();
    let _ = server.wait();
}

/// Regression: concurrent request-isolated traffic must acknowledge every
/// descriptor transfer before either side closes its copy.
#[test]
fn web_request_worker_survives_concurrent_connection_load() {
    let dir = make_test_dir("web_request_dispatch_ack");
    let bin = compile_isolated_web(&dir, "<?php echo 'ok';", "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server_with_args(
        &bin,
        &addr,
        "1",
        &["--handler-concurrency", "8"],
    );
    let worker = wait_for_direct_child_count(server.id(), 1)[0];
    let clients = (0..8)
        .map(|_| {
            let addr = addr.clone();
            std::thread::spawn(move || {
                for request in 0..50 {
                    let response = http_get(&addr, "/");
                    assert!(
                        response.starts_with("HTTP/1.1 200")
                            && raw_response_body(&response).contains("ok"),
                        "concurrent request-isolated request {request} failed: {response:?}"
                    );
                }
            })
        })
        .collect::<Vec<_>>();
    for client in clients {
        client.join().expect("concurrent request client panicked");
    }

    assert_eq!(
        wait_for_direct_child_count(server.id(), 1),
        vec![worker],
        "concurrent request load unexpectedly recycled the web worker"
    );
    let _ = server.kill();
    let _ = server.wait();
}

/// Reproduces disconnecting no-output handlers and proves request PIDs are cancelled and reaped.
#[test]
fn web_request_disconnect_cancels_and_reaps_no_output_handlers() {
    let dir = make_test_dir("web_request_disconnect_cancel");
    let src = r#"<?php
if (($_SERVER['REQUEST_URI'] ?? '') === '/hang') { while (true) {} }
echo 'ok';
"#;
    let bin = compile_isolated_web(&dir, src, "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server_with_args(
        &bin,
        &addr,
        "1",
        &["--handler-concurrency", "2", "--max-execution-time", "0"],
    );
    let worker = wait_for_direct_child_count(server.id(), 1)[0];
    let broker = wait_for_direct_child_count(worker, 1)[0];

    for _ in 0..8 {
        let mut stream = TcpStream::connect(&addr).expect("connect no-output handler");
        stream
            .write_all(
                format!("GET /hang HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .expect("dispatch no-output handler");
        drop(stream);
        assert!(
            direct_child_process_ids(broker).len() <= 2,
            "request broker exceeded --handler-concurrency after disconnect"
        );
    }
    wait_for_direct_child_count(broker, 0);
    let healthy = http_get(&addr, "/");
    assert!(
        raw_response_body(&healthy).contains("ok"),
        "request broker did not recover after cancellations: {healthy:?}"
    );

    let _ = server.kill();
    let _ = server.wait();
}

/// Verifies killing a worker's dedicated handler broker does not leave that
/// worker permanently returning failures or hanging subsequent requests.
#[test]
fn web_worker_recovers_after_handler_broker_death() {
    let dir = make_test_dir("web_broker_recovery");
    let bin = compile_isolated_web(&dir, "<?php echo 'alive';", "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server(&bin, &addr, "1");
    let baseline = http_get(&addr, "/");
    assert!(
        baseline.starts_with("HTTP/1.1 200") && baseline.contains("alive"),
        "broker fixture did not serve its baseline request: {baseline:?}"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let (worker_pid, broker_pid) = loop {
        let workers = direct_child_process_ids(server.id());
        if let Some(worker_pid) = workers.first().copied() {
            let brokers = direct_child_process_ids(worker_pid);
            if let Some(broker_pid) = brokers.first().copied() {
                break (worker_pid, broker_pid);
            }
        }
        assert!(Instant::now() < deadline, "handler broker did not start");
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_ne!(worker_pid, broker_pid, "worker and broker PIDs must differ");
    let killed = unsafe { libc::kill(broker_pid as libc::pid_t, libc::SIGKILL) };
    assert_eq!(killed, 0, "failed to kill handler broker");

    let deadline = Instant::now() + Duration::from_secs(8);
    let recovered = loop {
        if let Ok(response) = http_get_with_timeout(&addr, "/", Duration::from_millis(500)) {
            if response.starts_with("HTTP/1.1 200") && response.contains("alive") {
                break true;
            }
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let _ = server.kill();
    let _ = server.wait();
    assert!(recovered, "web worker did not recover after its broker died");
}

/// Verifies per-request reset of top-level PHP variables between two real HTTP
/// requests: each response body must be exactly "x" (not accumulated).
#[test]
fn web_reset_clears_globals_between_runs() {
    let dir = make_test_dir("web_reset");
    let src = "<?php $g = \"\"; $g = $g . \"x\"; echo $g;";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("x") || r1.ends_with("x\r\n0\r\n\r\n"),
        "first response body: {:?}",
        r1
    );
    assert!(
        r2.ends_with("x") || r2.ends_with("x\r\n0\r\n\r\n"),
        "second response body: {:?}",
        r2
    );
}

/// Verifies per-request reset of an ordinary global used through `global $g`.
#[test]
fn web_reset_clears_ordinary_global_alias_between_requests() {
    let dir = make_test_dir("web_reset_global_alias");
    let src = r#"<?php
function write_global(): void { global $g; $g = 7; }
function read_global(): int { global $g; return $g ?? 0; }
if (isset($_GET["set"])) { write_global(); }
echo read_global();
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/?set=1");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("7"), "first response body: {:?}", r1);
    assert!(r2.ends_with("0"), "second response leaked ordinary global: {:?}", r2);
}

/// Verifies per-request reset of a function static: each request must see
/// the static re-initialized to 0, so each response ends with "1".
#[test]
fn web_reset_clears_function_static() {
    let dir = make_test_dir("web_reset_static");
    let src = "<?php function c() { static $n = 0; $n++; return $n; } echo c();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first response body: {:?}", r1);
    assert!(r2.ends_with("1"), "second response body: {:?}", r2);
}

/// Verifies per-request reset of a static class property: each request must see
/// the property re-initialized to 0, so each response ends with "1".
#[test]
fn web_reset_clears_static_property() {
    let dir = make_test_dir("web_reset_prop");
    let src = "<?php class C { public static int $n = 0; } C::$n = C::$n + 1; echo C::$n;";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first response body: {:?}", r1);
    assert!(r2.ends_with("1"), "second response body: {:?}", r2);
}

/// Verifies an output buffer left open at request end is flushed, and does not swallow
/// the responses that follow.
///
/// PHP flushes whatever `ob_start()` left open at request shutdown. The `--web`
/// epilogue skipped that drain, so the request itself returned nothing and the leaked
/// nesting level captured every later response served by the same worker.
#[test]
fn web_unbalanced_output_buffer_is_flushed_and_not_inherited() {
    let dir = make_test_dir("web_ob_leak");
    let src = r#"<?php
if (isset($_GET["leak"])) {
    ob_start();
    echo "buffered";
} else {
    echo "plain-ok";
}
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/?leak=1");
    let second = http_get(&addr, "/");
    let third = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        first.ends_with("buffered"),
        "an output buffer left open was not flushed at request end: {:?}",
        first
    );
    assert!(
        second.ends_with("plain-ok"),
        "a leaked output-buffer level swallowed the next response: {:?}",
        second
    );
    assert!(third.ends_with("plain-ok"), "third response body: {:?}", third);
}

/// Verifies a worker survives reading the wrapper registry after an earlier request
/// registered one.
///
/// `stream_wrapper_register()` keeps its tables in the PHP arena, which the per-request
/// reset wipes, but reaches them through process-lifetime pointers. Left dangling, the
/// next `stream_get_wrappers()` read a garbage slot count and allocated until the heap
/// was exhausted, killing the worker — the response simply never arrived.
#[test]
fn web_wrapper_registry_does_not_outlive_the_request_arena() {
    let dir = make_test_dir("web_wrapper_registry_arena");
    let src = r#"<?php
class ArenaWrapper {
    public $context;
    public function stream_open($path, $mode, $options, &$openedPath): bool { return true; }
    public function stream_close(): void {}
}

if (isset($_GET["register"])) {
    stream_wrapper_register("arena.probe", "ArenaWrapper");
    echo "registered";
} else {
    echo "wrappers=", (int) in_array("php", stream_get_wrappers(), true);
}
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/?register=1");
    let second = http_get(&addr, "/");
    let third = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(first.ends_with("registered"), "first response body: {:?}", first);
    assert!(
        second.ends_with("wrappers=1"),
        "reading the wrapper registry after a registration killed the worker: {:?}",
        second
    );
    assert!(third.ends_with("wrappers=1"), "third response body: {:?}", third);
}

/// Verifies a worker survives a request that grows the resource registry past its
/// static slots, and the one after it.
///
/// The registry starts in a static eight-slot block and grows onto the PHP arena. That
/// block hid the defect for small requests; past eight resources the next request
/// walked slots the arena reset had already reclaimed.
#[test]
fn web_resource_registry_growth_survives_the_request_arena() {
    let dir = make_test_dir("web_resource_registry_growth");
    let src = r#"<?php
$streams = [];
for ($i = 0; $i < 40; $i++) {
    $streams[] = fopen("php://memory", "r+");
}
echo "opened=", count($streams);
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let second = http_get(&addr, "/");
    let third = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(first.ends_with("opened=40"), "first response body: {:?}", first);
    assert!(
        second.ends_with("opened=40"),
        "a grown resource registry did not survive the request arena reset: {:?}",
        second
    );
    assert!(third.ends_with("opened=40"), "third response body: {:?}", third);
}

/// Verifies request reset closes an abandoned user-wrapper resource exactly
/// once before the next request runs in the same worker process.
#[test]
fn web_stream_registry_request_reset_closes_abandoned_resource_once() {
    let dir = make_test_dir("web_stream_registry_reset");
    let count_file = dir
        .join("close-count.txt")
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let src = r#"<?php
class RequestCloseWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_close(): void {
        $count = file_exists("__COUNT_FILE__")
            ? (int) file_get_contents("__COUNT_FILE__")
            : 0;
        file_put_contents("__COUNT_FILE__", (string) ($count + 1));
    }
}

if (!in_array("reqclose", stream_get_wrappers(), true)) {
    stream_wrapper_register("reqclose", "RequestCloseWrapper");
}

if (isset($_GET["hold"])) {
    $heldStream = fopen("reqclose://request", "r");
    echo is_resource($heldStream) ? "held" : "open-failed";
} else {
    echo "closed=";
    echo file_exists("__COUNT_FILE__")
        ? file_get_contents("__COUNT_FILE__")
        : "0";
}
"#
    .replace("__COUNT_FILE__", &count_file);
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/?hold=1");
    let second = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(first.ends_with("held"), "first response body: {:?}", first);
    assert!(
        second.ends_with("closed=1"),
        "second response did not observe exact-once stream cleanup: {:?}",
        second
    );
}

/// Verifies that "Hello World" is served as the response body.
#[test]
fn web_server_serves_echo_body() {
    let dir = make_test_dir("web_echo");
    let bin = compile_web(&dir, "<?php echo \"Hello World\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("Hello World"), "response: {:?}", resp);
}

/// Verifies that the binary exits nonzero and prints "--listen" to stderr when
/// no --listen argument is supplied.
#[test]
fn web_server_requires_listen() {
    let dir = make_test_dir("web_nolisten");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let output = Command::new(&bin)
        .output()
        .expect("failed to spawn web binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected nonzero exit when --listen is missing");
    assert!(stderr.contains("--listen"), "expected --listen in stderr: {:?}", stderr);
}

/// Verifies that with --workers 2, two sequential requests both succeed and
/// each response ends with "ok".
#[test]
fn web_server_multiple_workers() {
    let dir = make_test_dir("web_multi");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "2");
    let r1 = http_get(&addr, "/");
    let r2 = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("ok"), "first response: {:?}", r1);
    assert!(r2.ends_with("ok"), "second response: {:?}", r2);
}

/// Sends one HTTP/1.1 request and returns it with any complete chunked body decoded.
fn http_request(addr: &str, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> String {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, addr);
    for (k, v) in headers { req.push_str(&format!("{}: {}\r\n", k, v)); }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    normalize_complete_http_response(buf)
}

/// Like `http_request` GET, but tolerates a refused/reset connection (returns the
/// empty string). Used while a worker is crashing/respawning.
fn try_http_get(addr: &str, path: &str) -> String {
    use std::io::{Read, Write};
    let Ok(mut s) = std::net::TcpStream::connect(addr) else {
        return String::new();
    };
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    if s.write_all(req.as_bytes()).is_err() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    normalize_complete_http_response(buf)
}

/// Verifies the extern getters are callable from --web PHP and return request data.
#[test]
fn web_extern_method_getter() {
    let dir = make_test_dir("web_extern_method");
    let bin = compile_web(&dir, "<?php echo elephc_web_method();", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "POST", "/", &[], "");
    let _ = child.kill(); let _ = child.wait();
    assert!(resp.ends_with("POST"), "body: {:?}", resp);
}

/// Verifies a superglobal is READABLE inside a function without `global` (full
/// visibility + global-storage routing). Now that $_SERVER is populated by the
/// prelude, asserts the body is the actual REQUEST_METHOD ("DELETE").
#[test]
fn web_superglobal_visible_in_function() {
    let dir = make_test_dir("web_sg_fn");
    let src = "<?php function rm() { return $_SERVER['REQUEST_METHOD'] ?? 'unset'; } echo rm();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "DELETE", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("DELETE"), "body: {:?}", resp);
}

/// Verifies a router storing an interface-typed handler survives repeated web requests.
#[test]
fn web_router_interface_handler_survives_repeated_requests() {
    let dir = make_test_dir("web_router_iface");
    let src = r#"<?php
class Request {
    public string $method;
    public string $path;
    public function __construct() {
        $this->method = $_SERVER['REQUEST_METHOD'] ?? 'GET';
        $uri = $_SERVER['REQUEST_URI'] ?? '/';
        $cut = strpos($uri, '?');
        $this->path = $cut === false ? $uri : substr($uri, 0, $cut);
    }
    public function segment(int $index, string $default = ''): string {
        $n = 0;
        foreach (explode('/', $this->path) as $part) {
            if ($part === '') { continue; }
            if ($n === $index) { return $part; }
            $n++;
        }
        return $default;
    }
}
interface Handler { public function handle(Request $request): void; }
class Hello implements Handler {
    public function handle(Request $request): void {
        echo 'Hello, ' . $request->segment(1, 'world') . "\n";
    }
}
class Route {
    public string $method;
    public string $pattern;
    public Handler $handler;
    public function __construct(string $method, string $pattern, Handler $handler) {
        $this->method = $method;
        $this->pattern = $pattern;
        $this->handler = $handler;
    }
    public function matches(Request $request): bool {
        return $this->method === $request->method;
    }
    public function run(Request $request): void {
        $this->handler->handle($request);
    }
}
class Router {
    private array $routes = [];
    public function add(string $method, string $pattern, Handler $handler): void {
        $this->routes[] = new Route($method, $pattern, $handler);
    }
    public function dispatch(Request $request): void {
        foreach ($this->routes as $route) {
            if (!$route->matches($request)) { continue; }
            $route->run($request);
            return;
        }
        echo 'missing';
    }
}
$router = new Router();
$router->add('GET', '/hello/:name', new Hello());
$router->dispatch(new Request());
"#;
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    for i in 0..25 {
        let resp = http_get(&addr, "/hello/ada");
        assert!(
            resp.ends_with("Hello, ada\n"),
            "response {i} body: {:?}",
            resp
        );
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies $_SERVER is populated from the request line and headers.
#[test]
fn web_server_superglobal_populated() {
    let dir = make_test_dir("web_server_sg");
    let src = "<?php echo $_SERVER['REQUEST_METHOD'] . ' ' . $_SERVER['REQUEST_URI'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port(); let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/foo?a=1", &[], "");
    let _ = child.kill(); let _ = child.wait();
    assert!(resp.ends_with("GET /foo?a=1"), "body: {:?}", resp);
}

/// Verifies $_GET is parsed from the query string, with percent-decoding.
#[test]
fn web_get_superglobal_parsed() {
    let dir = make_test_dir("web_get_sg");
    let src = "<?php echo $_GET['name'] . '/' . $_GET['city'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?name=bob&city=new%20york", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("bob/new york"), "body: {:?}", resp);
}

/// Verifies $_POST is parsed from a urlencoded body when the Content-Type matches.
#[test]
fn web_post_superglobal_parsed() {
    let dir = make_test_dir("web_post_sg");
    let src = "<?php echo $_POST['user'] . ':' . $_POST['pw'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "user=alice&pw=s%40fe",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("alice:s@fe"), "body: {:?}", resp);
}

/// Verifies echoing a superglobal value directly (a boxed Mixed string) reaches
/// the HTTP response body, not the worker's stdout. This is the output-capture
/// completeness fix: `__rt_mixed_write_stdout` routes through `__rt_stdout_write`.
#[test]
fn web_echo_superglobal_value_captured() {
    let dir = make_test_dir("web_mixed_cap");
    let src = "<?php echo $_GET['name'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?name=bob", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("bob"), "Mixed echo must be captured: {:?}", resp);
}

/// Verifies request superglobals do not leak/stale across requests: a second
/// request with a different query sees only its own $_GET (__rt_web_reset
/// releases the prior request's hash so there is no per-request leak).
#[test]
fn web_get_does_not_leak_across_requests() {
    let dir = make_test_dir("web_get_leak");
    let src = "<?php echo isset($_GET['a']) ? $_GET['a'] : 'none';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_request(&addr, "GET", "/?a=first", &[], "");
    let r2 = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("first"), "r1: {:?}", r1);
    assert!(r2.ends_with("none"), "r2 leaked stale $_GET: {:?}", r2);
}

/// Verifies file_get_contents('php://input') returns the raw request body under --web.
#[test]
fn web_php_input_returns_body() {
    let dir = make_test_dir("web_php_input");
    let src = "<?php echo file_get_contents('php://input');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", "application/json")], "{\"k\":42}");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("{\"k\":42}"), "body: {:?}", resp);
}

/// Verifies http_response_code() sets the HTTP response status.
#[test]
fn web_http_response_code_sets_status() {
    let dir = make_test_dir("web_status");
    let bin = compile_web(&dir, "<?php http_response_code(404); echo \"nope\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 404"), "status line: {:?}", resp);
    assert!(resp.ends_with("nope"), "body: {:?}", resp);
}

/// Verifies header() adds a response header (hyper lowercases header names on the wire).
#[test]
fn web_header_sets_response_header() {
    let dir = make_test_dir("web_header");
    let bin = compile_web(&dir, "<?php header(\"X-Greeting: hello\"); echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.to_lowercase().contains("x-greeting: hello"), "headers: {:?}", resp);
    assert!(resp.ends_with("ok"), "body: {:?}", resp);
}

/// Verifies header/status mutations after the first streamed byte are ignored
/// with PHP's "headers already sent" diagnostics instead of failing silently.
#[test]
fn web_late_header_and_status_emit_headers_already_sent_warnings() {
    let dir = make_test_dir("web_late_headers_warning");
    let bin = compile_isolated_web(
        &dir,
        "<?php echo 'body'; header('X-Late: ignored'); http_response_code(418);",
        "app",
        "request",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let stderr_path = dir.join("server.stderr");
    let stderr = fs::File::create(&stderr_path).expect("create late-header stderr capture");
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn late-header warning server");
    wait_until_ready(&addr);

    let response = http_request(&addr, "GET", "/", &[], "");
    let pid = child.id();
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    let _ = child.wait();
    let diagnostics = fs::read_to_string(&stderr_path).expect("read late-header diagnostics");

    assert!(response.starts_with("HTTP/1.1 200"), "late status changed response: {response:?}");
    assert!(!response.to_ascii_lowercase().contains("x-late:"));
    assert!(raw_response_body(&response).contains("body"));
    assert!(
        diagnostics.contains("header()") && diagnostics.contains("headers already sent"),
        "missing late header warning: {diagnostics:?}"
    );
    assert!(
        diagnostics.contains("http_response_code()")
            && diagnostics.matches("headers already sent").count() >= 2,
        "missing late response-code warning: {diagnostics:?}"
    );
}

/// Verifies header("Location: ...") implies a 302 redirect, matching PHP.
#[test]
fn web_header_location_implies_302() {
    let dir = make_test_dir("web_redirect");
    let bin = compile_web(&dir, "<?php header(\"Location: /elsewhere\");", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 302"), "status: {:?}", resp);
    assert!(resp.to_lowercase().contains("location: /elsewhere"), "headers: {:?}", resp);
}

/// Verifies http_response_code() + header() compose, function_exists sees them,
/// and the default $replace=true keeps only the last same-name header.
#[test]
fn web_response_control_combined() {
    let dir = make_test_dir("web_resp_combo");
    let src = "<?php \
        if (!function_exists('header') || !function_exists('http_response_code')) { echo 'MISSING'; return; } \
        http_response_code(201); \
        header('Content-Type: application/json'); \
        header('X-A: 1'); header('X-A: 2'); \
        echo '{\"ok\":true}';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let lower = resp.to_lowercase();
    assert!(resp.starts_with("HTTP/1.1 201"), "status: {:?}", resp);
    assert!(lower.contains("content-type: application/json"), "headers: {:?}", resp);
    assert!(lower.contains("x-a: 2") && !lower.contains("x-a: 1"), "replace failed: {:?}", resp);
    assert!(resp.ends_with("{\"ok\":true}"), "body: {:?}", resp);
}

/// Verifies a top-level `return` halts the --web handler: code after it must not run.
#[test]
fn web_top_level_return_halts_handler() {
    let dir = make_test_dir("web_return");
    let src = "<?php echo \"before\"; return; http_response_code(500); echo \"after\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.starts_with("HTTP/1.1 200"), "status must stay 200: {:?}", resp);
    assert!(resp.ends_with("before"), "body must be exactly 'before': {:?}", resp);
    assert!(!resp.contains("after"), "code after return must not run: {:?}", resp);
}

/// Verifies the validate-then-return pattern: a conditional early `return` halts
/// the handler so the rest of the body does not run (the failing case from the
/// web-response example).
#[test]
fn web_conditional_early_return_halts() {
    let dir = make_test_dir("web_early_return");
    let src = "<?php if (!isset($_GET['ok'])) { http_response_code(400); echo \"bad\"; return; } echo \"good\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let bad = http_request(&addr, "GET", "/", &[], "");
    let good = http_request(&addr, "GET", "/?ok=1", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(bad.starts_with("HTTP/1.1 400"), "no-ok status: {:?}", bad);
    assert!(bad.ends_with("bad"), "no-ok body must be 'bad': {:?}", bad);
    assert!(!bad.contains("good"), "no-ok must not run code after return: {:?}", bad);
    assert!(good.starts_with("HTTP/1.1 200"), "ok status: {:?}", good);
    assert!(good.ends_with("good"), "ok body must be 'good': {:?}", good);
}

/// Verifies `--gc-stats` emits one counter line after every web request,
/// including a request that leaves the top-level handler through early return.
#[test]
fn web_gc_stats_are_emitted_per_request() {
    let dir = make_test_dir("web_gc_stats");
    let src = "<?php if (!isset($_GET['ok'])) { echo 'early'; return; } echo 'normal';";
    let bin = compile_web_with_flags(&dir, src, "app", &["--gc-stats"]);
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let stderr_path = dir.join("server.stderr");
    let stderr_file = fs::File::create(&stderr_path).expect("create server stderr capture");
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1"])
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("spawn web server with gc stats");
    wait_until_ready(&addr);

    let early = http_request(&addr, "GET", "/", &[], "");
    let normal = http_request(&addr, "GET", "/?ok=1", &[], "");
    let pid = child.id();
    let signal = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("send SIGTERM to web server");
    assert!(signal.success(), "failed to stop web server");
    let status = child.wait().expect("wait for web server shutdown");
    assert_eq!(status.code(), Some(0), "web server must stop cleanly");

    assert!(early.ends_with("early"), "early-return response: {:?}", early);
    assert!(normal.ends_with("normal"), "normal response: {:?}", normal);
    let stderr = fs::read_to_string(&stderr_path).expect("read captured server stderr");
    let stats: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with("GC: allocs="))
        .collect();
    assert_eq!(
        stats.len(),
        2,
        "expected one gc-stats line per handled request, stderr: {}",
        stderr
    );
    for line in stats {
        let Some((allocs, frees)) = line
            .strip_prefix("GC: allocs=")
            .and_then(|rest| rest.split_once(" frees="))
        else {
            panic!("malformed gc-stats line: {}", line);
        };
        assert!(allocs.parse::<u64>().is_ok(), "invalid allocation count: {}", line);
        assert!(frees.parse::<u64>().is_ok(), "invalid free count: {}", line);
    }
}

/// Verifies a request body over --max-body-size is rejected with 413, and a body
/// under the limit is served normally.
#[test]
fn web_body_size_limit_returns_413() {
    let dir = make_test_dir("web_bodylimit");
    let src = "<?php echo strlen(file_get_contents('php://input'));";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = ServerGuard::new(
        Command::new(&bin)
            .args(["--listen", &addr, "--workers", "1", "--max-body-size", "64"])
            .spawn()
            .expect("spawn"),
    );
    wait_until_ready(&addr);
    let small = http_request(&addr, "POST", "/", &[("Content-Type", "text/plain")], &"x".repeat(10));
    let big = http_request(&addr, "POST", "/", &[("Content-Type", "text/plain")], &"x".repeat(1000));
    let _ = child.kill();
    let _ = child.wait();
    assert!(small.ends_with("10"), "under-limit body should serve: {:?}", small);
    assert!(big.starts_with("HTTP/1.1 413"), "over-limit body should be 413: {:?}", big);
}

/// Verifies a client that advertises a body and then trickles no remaining
/// bytes is terminated by the configured body-read deadline with HTTP 408.
#[test]
fn web_body_read_timeout_returns_408_for_trickled_request() {
    let dir = make_test_dir("web_body_timeout");
    let src = "<?php echo strlen(file_get_contents('php://input'));";
    let bin = compile_isolated_web(&dir, src, "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args([
            "--listen",
            &addr,
            "--workers",
            "1",
            "--body-read-timeout",
            "1",
        ])
        .spawn()
        .expect("spawn body-timeout server");
    wait_until_ready(&addr);

    let mut stream = TcpStream::connect(&addr).expect("connect trickle client");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(
            b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1000\r\nConnection: close\r\n\r\nx",
        )
        .unwrap();
    let mut response = String::new();
    let read = stream.read_to_string(&mut response);

    let _ = child.kill();
    let _ = child.wait();
    assert!(read.is_ok(), "body timeout must close the connection: {read:?}");
    assert!(
        response.starts_with("HTTP/1.1 408"),
        "trickled body should receive 408, got: {response:?}"
    );
}

/// Verifies one slow PHP handler does not head-of-line block an unrelated
/// request accepted by the same web worker.
#[test]
fn web_worker_keeps_serving_while_another_handler_is_slow() {
    for mode in ["pool", "request"] {
        let dir = make_test_dir(&format!("web_handler_concurrency_{mode}"));
        let src = r#"<?php
if (($_GET['slow'] ?? '0') === '1') { usleep(1000000); }
echo $_GET['name'] ?? 'missing';
"#;
        let bin = compile_isolated_web(&dir, src, "app", mode);
        let port = free_port();
        let addr = format!("127.0.0.1:{}", port);
        let mut child = spawn_server_with_args(
            &bin,
            &addr,
            "1",
            &["--handler-concurrency", "2"],
        );

        let slow_addr = addr.clone();
        let slow = std::thread::spawn(move || {
            http_request(&slow_addr, "GET", "/?slow=1&name=slow", &[], "")
        });
        std::thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        let quick = http_request(&addr, "GET", "/?name=quick", &[], "");
        let quick_elapsed = started.elapsed();
        let slow_response = slow.join().expect("slow request thread");

        let _ = child.kill();
        let _ = child.wait();
        assert!(
            raw_response_body(&quick).contains("quick"),
            "{mode} quick response was not served: {quick:?}"
        );
        assert!(
            raw_response_body(&slow_response).contains("slow"),
            "{mode} slow response was not served: {slow_response:?}"
        );
        assert!(
            quick_elapsed < Duration::from_millis(500),
            "{mode} handler stalled an unrelated request for {quick_elapsed:?}"
        );
    }
}

/// Guards the prestarted broker's steady-state fork/IPC overhead with a small
/// sequential request sample that excludes compilation and server startup.
#[test]
fn web_handler_broker_dispatch_overhead_stays_bounded() {
    let dir = make_test_dir("web_broker_dispatch_benchmark");
    let bin = compile_isolated_web(&dir, "<?php echo 'ok';", "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut child = spawn_server(&bin, &addr, "1");

    let started = Instant::now();
    for sample in 0..25 {
        let response = http_request(&addr, "GET", "/", &[], "");
        assert!(
            raw_response_body(&response).contains("ok"),
            "broker benchmark request {sample} failed: {response:?}"
        );
    }
    let elapsed = started.elapsed();

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        elapsed < Duration::from_secs(5),
        "25 isolated handler dispatches took {elapsed:?}"
    );
}

/// Verifies response bytes reach the client before a slow handler completes,
/// avoiding full-response buffering and a second whole-body IPC copy.
#[test]
fn web_response_streams_before_handler_completion() {
    let dir = make_test_dir("web_response_streaming");
    let src = r#"<?php
echo str_repeat("A", 4096);
usleep(1000000);
echo "done";
"#;
    let bin = compile_isolated_web(&dir, src, "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut child = spawn_server(&bin, &addr, "1");

    let mut stream = TcpStream::connect(&addr).expect("connect streaming client");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("set bounded streaming read timeout");
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .expect("send streaming request");

    let deadline = Instant::now() + Duration::from_millis(700);
    let mut early = Vec::new();
    while Instant::now() < deadline && !early.windows(64).any(|bytes| bytes == [b'A'; 64]) {
        let mut chunk = [0u8; 8192];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => early.extend_from_slice(&chunk[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("streaming response read failed: {error}"),
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        early.windows(64).any(|bytes| bytes == [b'A'; 64]),
        "the first response chunk was still buffered until handler completion: {} bytes received",
        early.len()
    );
}

/// Verifies the server shuts down cleanly (exit code 0) on SIGTERM, promptly.
#[test]
fn web_sigterm_shuts_down_cleanly() {
    let dir = make_test_dir("web_sigterm");
    let bin = compile_web(&dir, "<?php echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "2");
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("ok"));
    let pid = child.id();
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().expect("try_wait") {
            break s;
        }
        if start.elapsed() > Duration::from_secs(8) {
            let _ = child.kill();
            panic!("master did not exit within 8s of SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "master should exit 0 on SIGTERM");
}

/// Verifies that a worker which dies mid-request is respawned, so the single-worker
/// server keeps serving subsequent requests.
#[test]
fn web_worker_respawns_after_crash() {
    let dir = make_test_dir("web_respawn");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') { exit(1); } echo \"alive\";";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("alive"));
    // Crash the only worker (the connection is dropped mid-handler).
    let _ = try_http_get(&addr, "/crash");
    // The master must respawn a worker; retry until / serves again.
    let mut served = false;
    for _ in 0..40 {
        if try_http_get(&addr, "/").ends_with("alive") {
            served = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(served, "worker was not respawned after a crash");
}

/// Verifies an isolated handler crash yields a bounded HTTP 500 and does not
/// poison the worker's ability to serve the following request.
#[test]
fn web_handler_crash_returns_500_and_next_request_survives() {
    for mode in ["pool", "request"] {
        let dir = make_test_dir(&format!("web_handler_crash_500_{mode}"));
        let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') { exit(1); } echo \"alive\";";
        let bin = compile_isolated_web(&dir, src, "app", mode);
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let mut child = spawn_server(&bin, &addr, "1");

        let failed = http_request(&addr, "GET", "/crash", &[], "");
        let healthy = http_request(&addr, "GET", "/", &[], "");

        let _ = child.kill();
        let _ = child.wait();
        assert!(
            failed.starts_with("HTTP/1.1 500"),
            "{mode} crashed handler did not produce an explicit 500: {failed:?}"
        );
        assert!(
            failed.ends_with("Internal Server Error"),
            "{mode} crashed handler returned an unbounded failure: {failed:?}"
        );
        assert!(
            raw_response_body(&healthy).contains("alive"),
            "{mode} worker did not recover after handler crash: {healthy:?}"
        );
    }
}

/// Verifies a handler that dies after committing output cannot be presented as
/// a cleanly terminated, cacheable chunked success response.
#[test]
fn web_handler_crash_after_commit_aborts_chunked_response() {
    let dir = make_test_dir("web_handler_crash_after_commit");
    let src = r#"<?php
if (($_SERVER['REQUEST_URI'] ?? '') === '/crash') {
    echo str_repeat("A", 4096);
    throw new Exception("after commit");
}
echo "healthy";
"#;
    let bin = compile_isolated_web(&dir, src, "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut child = spawn_server(&bin, &addr, "1");

    let mut stream = TcpStream::connect(&addr).expect("connect crash-after-commit client");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set crash-after-commit timeout");
    stream
        .write_all(
            format!(
                "GET /crash HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .expect("send crash-after-commit request");
    let failed = read_raw_response_allowing_abort(&mut stream);
    let healthy = http_request(&addr, "GET", "/", &[], "");

    let _ = child.kill();
    let _ = child.wait();
    assert!(
        failed.starts_with("HTTP/1.1 200"),
        "the fixture must commit its success status before crashing: {failed:?}"
    );
    assert!(
        raw_response_body(&failed).contains("AAAA"),
        "the fixture did not commit its first body chunk: {failed:?}"
    );
    assert!(
        !failed.ends_with("0\r\n\r\n"),
        "an incomplete handler response was terminated as a valid chunked success: {failed:?}"
    );
    assert!(
        raw_response_body(&healthy).contains("healthy"),
        "the broker did not survive a post-commit handler crash: {healthy:?}"
    );
}

/// Verifies a configured response-write inactivity timeout frees all handler
/// slots pinned by clients that request unbounded output and never read it.
#[test]
fn web_response_write_timeout_releases_stalled_handler_slots() {
    let dir = make_test_dir("web_response_write_timeout");
    let src = r#"<?php
if (($_SERVER['REQUEST_URI'] ?? '') === '/stall') {
    while (true) { echo str_repeat("S", 65536); }
}
echo "fast";
"#;
    let bin = compile_isolated_web(&dir, src, "app", "request");
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut child = Command::new(&bin)
        .args([
            "--listen",
            &addr,
            "--workers",
            "1",
            "--handler-concurrency",
            "8",
            "--response-write-timeout",
            "1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn response-timeout server");
    wait_until_ready(&addr);

    let mut stalled = Vec::new();
    for _ in 0..8 {
        let mut stream = TcpStream::connect(&addr).expect("connect stalled response client");
        stream
            .write_all(
                format!(
                    "GET /stall HTTP/1.1\r\nHost: {addr}\r\nConnection: keep-alive\r\n\r\n"
                )
                .as_bytes(),
            )
            .expect("send stalled response request");
        stalled.push(stream);
    }
    std::thread::sleep(Duration::from_millis(1500));

    let started = Instant::now();
    let mut fast = TcpStream::connect(&addr).expect("connect fast response client");
    fast.set_read_timeout(Some(Duration::from_secs(4)))
        .expect("set fast response timeout");
    fast.write_all(
        format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .expect("send fast response request");
    let response = read_raw_response_allowing_abort(&mut fast);

    drop(stalled);
    let pid = child.id();
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    let _ = child.wait();
    assert!(
        raw_response_body(&response).contains("fast"),
        "stalled clients kept every handler slot pinned: {response:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the fast request waited beyond the configured response-write timeout"
    );
}

/// Verifies HTTP/1.1 keep-alive: two requests on ONE TCP connection both succeed.
#[test]
fn web_keep_alive_reuses_connection() {
    use std::io::Write;
    let dir = make_test_dir("web_keepalive");
    let bin = compile_web(&dir, "<?php echo \"hi\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    wait_until_ready(&addr);
    let mut sock = TcpStream::connect(&addr).expect("connect");
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!("GET / HTTP/1.1\r\nHost: {}\r\n\r\n", addr);
    sock.write_all(req.as_bytes()).unwrap();
    let resp1 = read_complete_http_response(&mut sock);
    // Second request on the SAME socket (only works if keep-alive kept it open).
    sock.write_all(req.as_bytes()).unwrap();
    let resp2 = read_complete_http_response(&mut sock);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp1.contains("200") && resp1.contains("hi"), "resp1: {:?}", resp1);
    assert!(resp2.contains("200") && resp2.contains("hi"), "keep-alive reuse failed: {:?}", resp2);
}

/// Regression: a request with many query parameters must not corrupt $_GET. The
/// superglobal assoc array grows past its initial capacity; before the fix the
/// grown table pointer was not written back to global storage, corrupting the
/// array (count went wrong / the worker crashed). 30 params must all survive.
#[test]
fn web_get_many_params_not_corrupted() {
    let dir = make_test_dir("web_get_many");
    let src = "<?php echo count($_GET) . '|' . ($_GET['p29'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let mut query = String::from("/?");
    for i in 0..30 {
        query.push_str(&format!("p{}={}&", i, i));
    }
    let resp = http_request(&addr, "GET", &query, &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("30|29"), "many-param $_GET corrupted: {:?}", resp);
}

/// Verifies the extended $_SERVER keys (A1): REMOTE_ADDR, SERVER_PORT,
/// SERVER_PROTOCOL, REQUEST_SCHEME, SERVER_SOFTWARE, REQUEST_TIME.
#[test]
fn web_server_vars_populated() {
    let dir = make_test_dir("web_server_vars");
    let src = "<?php echo $_SERVER['REMOTE_ADDR'].'|'.$_SERVER['SERVER_PORT'].'|'\
        .$_SERVER['SERVER_PROTOCOL'].'|'.$_SERVER['REQUEST_SCHEME'].'|'\
        .$_SERVER['SERVER_SOFTWARE'].'|'.($_SERVER['REQUEST_TIME'] > 0 ? 't' : 'f');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let expected = format!("127.0.0.1|{}|HTTP/1.1|http|elephc|t", port);
    assert!(resp.ends_with(&expected), "expected {:?} at end of {:?}", expected, resp);
}

/// Verifies $_COOKIE (A2): the Cookie header is parsed into the superglobal,
/// values are percent-decoded.
#[test]
fn web_cookie_parsed() {
    let dir = make_test_dir("web_cookie");
    let src = "<?php echo ($_COOKIE['a'] ?? '?').'|'.($_COOKIE['b'] ?? '?').'|'.count($_COOKIE);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[("Cookie", "a=1; b=hello%20world")], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("1|hello world|2"), "cookie parse: {:?}", resp);
}

/// Verifies $_REQUEST (A4): merges $_GET then $_POST (POST overrides on collision).
#[test]
fn web_request_superglobal_merges_get_post() {
    let dir = make_test_dir("web_request_merge");
    let src = "<?php echo ($_REQUEST['x'] ?? '?').'|'.($_REQUEST['q'] ?? '?').'|'.count($_REQUEST);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(
        &addr,
        "POST",
        "/?x=g&q=1",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "x=p",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("p|1|2"), "$_REQUEST merge (POST overrides GET): {:?}", resp);
}

/// Verifies setcookie() (A3): emits a Set-Cookie response header (value
/// percent-encoded, attributes appended), and multiple calls produce multiple
/// headers (replace=false).
#[test]
fn web_setcookie_emits_header() {
    let dir = make_test_dir("web_setcookie");
    let src = "<?php setcookie('sid', 'ab c', 0, '/'); setcookie('x', 'y'); echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    let lower = resp.to_lowercase();
    assert!(lower.contains("set-cookie: sid=ab%20c; path=/"), "first cookie: {:?}", resp);
    assert!(lower.contains("set-cookie: x=y"), "second cookie: {:?}", resp);
    assert!(resp.ends_with("ok"), "body: {:?}", resp);
}

/// Verifies $_ENV (A7) is populated from the process environment.
#[test]
fn web_env_superglobal_populated() {
    let dir = make_test_dir("web_env");
    let src = "<?php echo ($_ENV['ELEPHC_WEB_TEST_ENV'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = ServerGuard::new(
        Command::new(&bin)
            .args(["--listen", &addr, "--workers", "1"])
            .env("ELEPHC_WEB_TEST_ENV", "present")
            .spawn()
            .expect("spawn"),
    );
    wait_until_ready(&addr);
    let resp = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("present"), "$_ENV not populated: {:?}", resp);
}

/// Verifies the produced binary answers --help and --version (exit 0) (D4).
#[test]
fn web_help_and_version() {
    let dir = make_test_dir("web_help");
    let bin = compile_web(&dir, "<?php echo 'x';", "app");
    let help = Command::new(&bin).arg("--help").output().expect("help");
    assert!(help.status.success(), "--help should exit 0");
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--listen"),
        "--help should describe --listen"
    );
    let help_text = String::from_utf8_lossy(&help.stdout);
    assert!(
        help_text.contains("respawn a worker") && !help_text.contains("--handler-concurrency"),
        "default --web help must describe the in-process worker model: {help_text}"
    );

    let request_bin = compile_isolated_web(&dir, "<?php echo 'x';", "request-app", "request");
    let request_help = Command::new(&request_bin)
        .arg("--help")
        .output()
        .expect("request help");
    let request_help_text = String::from_utf8_lossy(&request_help.stdout);
    assert!(request_help.status.success(), "isolated --help should exit 0");
    assert!(
        request_help_text.contains("--response-write-timeout")
            && request_help_text.contains("--handler-concurrency")
            && request_help_text.contains("timed-out handler process"),
        "request help must expose isolated-handler controls: {request_help_text}"
    );
    let ver = Command::new(&bin).arg("--version").output().expect("version");
    assert!(ver.status.success(), "--version should exit 0");
    assert!(
        String::from_utf8_lossy(&ver.stdout).to_lowercase().contains("elephc-web"),
        "--version should name elephc-web"
    );
    // Missing --listen is a usage error (non-zero exit).
    let none = Command::new(&bin).output().expect("noargs");
    assert!(!none.status.success(), "missing --listen must exit non-zero");
}

/// Verifies --max-requests recycles a single worker yet the server keeps serving
/// across the recycle (the master respawns it) (B5).
#[test]
fn web_max_requests_recycles_and_keeps_serving() {
    let dir = make_test_dir("web_maxreq");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = ServerGuard::new(
        Command::new(&bin)
            .args(["--listen", &addr, "--workers", "1", "--max-requests", "2"])
            .spawn()
            .expect("spawn"),
    );
    wait_until_ready(&addr);
    // More requests than the cap: the server must keep serving across recycles.
    // A single-worker recycle has a brief no-listener window, so tolerate transient
    // connection-refused and retry — every logical request must eventually succeed.
    for _ in 0..6 {
        let mut ok = false;
        for _ in 0..40 {
            if try_http_get(&addr, "/").ends_with("ok") {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(ok, "server stopped serving across a --max-requests recycle");
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies every isolation model stops accepting and closes an idle keep-alive
/// connection once its completed request reaches the recycle quota.
#[test]
fn web_max_requests_drains_keep_alive_before_recycle() {
    for mode in ["worker", "pool", "request"] {
        let dir = make_test_dir(&format!("web_{mode}_maxreq_keepalive"));
        let bin = if mode == "worker" {
            compile_web(&dir, "<?php echo 'ok';", "app")
        } else {
            compile_isolated_web(&dir, "<?php echo 'ok';", "app", mode)
        };
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let mut child = ServerGuard::new(
            Command::new(&bin)
                .args([
                    "--listen",
                    &addr,
                    "--workers",
                    "1",
                    "--max-requests",
                    "1",
                ])
                .spawn()
                .expect("spawn isolated web server"),
        );
        wait_until_ready(&addr);

        let mut socket = TcpStream::connect(&addr).expect("connect keep-alive client");
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let request = format!("GET / HTTP/1.1\r\nHost: {addr}\r\n\r\n");
        socket.write_all(request.as_bytes()).unwrap();
        let first = read_complete_http_response(&mut socket);
        assert!(
            first.contains("200") && first.ends_with("ok"),
            "{mode} first response failed: {first:?}"
        );

        let mut trailing = [0u8; 1];
        assert_eq!(
            socket.read(&mut trailing).expect("wait for graceful keep-alive close"),
            0,
            "{mode} worker left the keep-alive connection open after its quota"
        );

        let mut served_after_recycle = false;
        for _ in 0..40 {
            if try_http_get(&addr, "/").ends_with("ok") {
                served_after_recycle = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            served_after_recycle,
            "{mode} master did not replace the recycled worker"
        );

        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Regression for issue #516 (caveat 2): planned --max-requests recycles used to
/// count toward the master's fast-death crash-loop guard, so sustained traffic
/// recycled a worker >10 times in under a second each and the master printed
/// "giving up" and shut the whole server down. Drive enough requests through a
/// tiny quota to force well past MAX_FAST_DEATHS (10) recycles and assert the
/// master is still alive and serving at the end.
#[test]
fn web_max_requests_survives_sustained_recycle_churn() {
    let dir = make_test_dir("web_maxreq_churn");
    let bin = compile_web(&dir, "<?php echo 'ok';", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = Command::new(&bin)
        .args(["--listen", &addr, "--workers", "1", "--max-requests", "2"])
        .spawn()
        .expect("spawn");
    wait_until_ready(&addr);
    // 30 requests with a quota of 2 forces ~15 recycles (> MAX_FAST_DEATHS).
    // Recycling is not graceful, so tolerate transient failures at recycle
    // boundaries — but every logical request must eventually succeed.
    for i in 0..30 {
        let mut ok = false;
        for _ in 0..40 {
            if try_http_get(&addr, "/").ends_with("ok") {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(ok, "server gave up during recycle churn at request {}", i + 1);
    }
    // The master must still be running (it must NOT have printed "giving up"
    // and exited after 10 fast recycles).
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "master exited during sustained --max-requests recycle churn"
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Verifies an uncaught exception in the handler returns HTTP 500 instead of
/// crashing the worker / dropping the connection (B1), and the server keeps
/// serving other requests afterward.
#[test]
fn web_uncaught_exception_returns_500() {
    let dir = make_test_dir("web_500");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/boom') { throw new Exception('kaboom'); } echo 'ok';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let ok = http_request(&addr, "GET", "/", &[], "");
    let boom = http_request(&addr, "GET", "/boom", &[], "");
    let after = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(raw_response_body(&ok).contains("ok"), "normal request: {:?}", ok);
    assert!(boom.starts_with("HTTP/1.1 500"), "uncaught exception must be 500: {:?}", boom);
    assert!(
        raw_response_body(&after).contains("ok"),
        "server must keep serving after a 500: {:?}",
        after
    );
}

/// Verifies --max-execution-time kills a runaway handler (and the master respawns
/// the worker so the server recovers) (B3).
#[test]
fn web_max_execution_time_kills_runaway_handler() {
    let dir = make_test_dir("web_exectime");
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/slow') { while (true) {} } echo 'fast';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = ServerGuard::new(
        Command::new(&bin)
            .args(["--listen", &addr, "--workers", "1", "--max-execution-time", "1"])
            .spawn()
            .expect("spawn"),
    );
    wait_until_ready(&addr);
    assert!(http_request(&addr, "GET", "/", &[], "").ends_with("fast"));
    // The runaway request is killed by the watchdog (dropped connection); tolerate it.
    let _ = try_http_get(&addr, "/slow");
    // The master must respawn the worker; / serves again within a few seconds.
    let mut recovered = false;
    for _ in 0..40 {
        if try_http_get(&addr, "/").ends_with("fast") {
            recovered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(recovered, "worker did not recover after a runaway handler was killed");
}

/// Verifies isolated timeouts replace only the handler process, preserving worker and broker PIDs.
#[test]
fn web_isolated_max_execution_time_preserves_worker_and_broker() {
    let src = "<?php if (($_SERVER['REQUEST_URI'] ?? '') === '/slow') { while (true) {} } echo 'fast';";
    for mode in ["pool", "request"] {
        let dir = make_test_dir(&format!("web_isolated_timeout_{mode}"));
        let bin = compile_isolated_web(&dir, src, "app", mode);
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let mut server = spawn_server_with_args(
            &bin,
            &addr,
            "1",
            &["--handler-concurrency", "1", "--max-execution-time", "1"],
        );
        let worker = wait_for_direct_child_count(server.id(), 1)[0];
        let broker = wait_for_direct_child_count(worker, 1)[0];
        assert!(raw_response_body(&http_get(&addr, "/")).contains("fast"));

        let _ = http_get_with_timeout(&addr, "/slow", Duration::from_secs(3));
        let deadline = Instant::now() + Duration::from_secs(5);
        let recovered = loop {
            if let Ok(response) = http_get_with_timeout(&addr, "/", Duration::from_millis(500)) {
                if raw_response_body(&response).contains("fast") {
                    break true;
                }
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(recovered, "{mode} did not recover after handler timeout");
        assert_eq!(
            wait_for_direct_child_count(server.id(), 1)[0],
            worker,
            "{mode} timeout recycled the web worker"
        );
        assert_eq!(
            wait_for_direct_child_count(worker, 1)[0],
            broker,
            "{mode} timeout recycled the handler broker"
        );

        let _ = server.kill();
        let _ = server.wait();
    }
}

/// Verifies --gzip compresses the response when the client sends Accept-Encoding:
/// gzip (and only then) (C3).
#[test]
fn web_gzip_compresses_when_accepted() {
    let dir = make_test_dir("web_gzip");
    let bin = compile_web(
        &dir,
        "<?php header('Content-Length: 2000'); echo str_repeat('ABCD', 500);",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = ServerGuard::new(
        Command::new(&bin)
            .args(["--listen", &addr, "--workers", "1", "--gzip"])
            .spawn()
            .expect("spawn"),
    );
    wait_until_ready(&addr);
    // The gzipped body is binary, so read raw bytes and inspect the (ASCII) header
    // block rather than http_request's read_to_string.
    let gz_head = {
        use std::io::{Read, Write};
        let mut sock = TcpStream::connect(&addr).unwrap();
        let req = format!(
            "GET / HTTP/1.1\r\nHost: {}\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n",
            addr
        );
        sock.write_all(req.as_bytes()).unwrap();
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf).unwrap();
        String::from_utf8_lossy(&buf[..buf.len().min(512)]).to_string()
    };
    let plain = http_request(&addr, "GET", "/", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(gz_head.to_lowercase().contains("content-encoding: gzip"), "gzip not applied: {:?}", gz_head);
    assert!(
        !gz_head.to_lowercase().contains("content-length: 2000"),
        "gzip retained the handler's uncompressed Content-Length: {gz_head:?}"
    );
    assert!(!plain.to_lowercase().contains("content-encoding"), "must not compress without Accept-Encoding");
    // The uncompressed response carries the full 2000-byte body.
    assert!(plain.ends_with(&"ABCD".repeat(500)), "plain body mismatch");
}

/// Verifies multipart/form-data parsing (A5): text fields land in $_POST and file
/// uploads populate $_FILES (name, type, size). The request is built by hand to
/// avoid depending on a multipart client.
#[test]
fn web_multipart_post_and_files() {
    let dir = make_test_dir("web_multipart");
    let src = "<?php echo ($_POST['greeting'] ?? '?').'|'.($_FILES['upload']['name'] ?? '?')\
        .'|'.($_FILES['upload']['type'] ?? '?').'|'.($_FILES['upload']['size'] ?? '?');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let boundary = "Xbnd";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"greeting\"\r\n\r\nhello\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"up.txt\"\r\n\
         Content-Type: text/plain\r\n\r\nFILEDATA-123\r\n--{b}--\r\n",
        b = boundary
    );
    let ct = format!("multipart/form-data; boundary={}", boundary);
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", &ct)], &body);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hello|up.txt|text/plain|12"), "multipart parse: {:?}", resp);
}

/// Verifies an uploaded file can be READ back via file_get_contents on its
/// tmp_name. This exercises both A5 and the multi-bridge link fix: a dynamic
/// file_get_contents pulls in the TLS bridge, which must co-link with the web
/// bridge without duplicate-symbol errors.
#[test]
fn web_multipart_file_contents_readable() {
    let dir = make_test_dir("web_upload_read");
    let src = "<?php $f = $_FILES['doc']['tmp_name'] ?? ''; echo $f === '' ? 'NOFILE' : file_get_contents($f);";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let boundary = "Zbnd";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"d.txt\"\r\n\
         Content-Type: application/octet-stream\r\n\r\nUPLOAD-CONTENT-OK\r\n--{b}--\r\n",
        b = boundary
    );
    let ct = format!("multipart/form-data; boundary={}", boundary);
    let resp = http_request(&addr, "POST", "/", &[("Content-Type", &ct)], &body);
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("UPLOAD-CONTENT-OK"), "upload content not read back: {:?}", resp);
}

/// Verifies a namespaced --web program (classes under a namespace) compiles and
/// serves. The B1 uncaught-exception wrap must not reorder top-level namespace
/// declarations away from the classes they scope (it skips the wrap entirely when
/// namespaces are present). Regression for the web-framework example.
#[test]
fn web_namespaced_program_serves() {
    let dir = make_test_dir("web_namespaced");
    let src = "<?php namespace App; \
        class Greeter { public function hi(string $n): string { return 'hi ' . $n; } } \
        $g = new Greeter(); echo $g->hi($_GET['n'] ?? 'world');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_request(&addr, "GET", "/?n=ada", &[], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("hi ada"), "namespaced --web program: {:?}", resp);
}

/// Verifies the OPcache `opcache_get_status()` prelude function under `--web`, where the
/// cache is enabled (`opcache.enable` default). Spot-checks the enabled status array:
/// `opcache_enabled` true; the class-B memory invariant (used + free + wasted ==
/// `opcache.memory_consumption` = 134217728) and the interned-strings invariant
/// (used + free == buffer_size); `opcache_statistics.max_cached_keys` == 16229 (derived
/// from the default `max_accelerated_files`); `opcache_hit_rate` == 0.0; `jit.enabled`
/// false (default `opcache.jit = disable`); the `scripts` key present for the default
/// call but ABSENT for `opcache_get_status(false)`; and `start_time` a live `time()`.
/// The expected string matches reference PHP `opcache_get_status()` run with the cache
/// enabled.
#[test]
fn web_opcache_get_status_reports_enabled_array() {
    let dir = make_test_dir("web_opcache_status");
    let src = "<?php \
$s = opcache_get_status(); \
$ns = opcache_get_status(false); \
echo ($s['opcache_enabled'] ? 'EN1' : 'EN0'), ':'; \
echo (($s['memory_usage']['used_memory'] + $s['memory_usage']['free_memory'] + $s['memory_usage']['wasted_memory']) == 134217728 ? 'MEMOK' : 'MEMBAD'), ':'; \
echo (($s['interned_strings_usage']['used_memory'] + $s['interned_strings_usage']['free_memory']) == $s['interned_strings_usage']['buffer_size'] ? 'INTOK' : 'INTBAD'), ':'; \
echo ($s['opcache_statistics']['max_cached_keys'] == 16229 ? 'MCK1' : 'MCK0'), ':'; \
echo ($s['opcache_statistics']['opcache_hit_rate'] == 0 ? 'HR1' : 'HR0'), ':'; \
echo ($s['jit']['enabled'] ? 'JIT0' : 'JIT1'), ':'; \
echo (isset($s['scripts']) ? 'SCR1' : 'SCR0'), ':'; \
echo (isset($ns['scripts']) ? 'NSCR1' : 'NSCR0'), ':'; \
echo ($s['opcache_statistics']['start_time'] > 1000000000 ? 'ST1' : 'ST0');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("EN1:MEMOK:INTOK:MCK1:HR1:JIT1:SCR1:NSCR0:ST1"),
        "opcache_get_status --web array mismatch: {:?}",
        resp
    );
}

/// Verifies the OPcache surface keeps the SAME SHAPE on every request of a long-lived worker.
///
/// This is the `--web` half of OPcache verification, and it deliberately uses NO reference PHP.
/// FPM would be the only oracle for cross-request behaviour, and it is out of scope on purpose
/// (see `docs/php/opcache.md`): elephc replaces FPM rather than plugging into it, and the
/// cross-request numbers FPM would expose — accumulating `hits`, a growing `scripts` map,
/// `opcache_reset()`'s deferred restart — are class-B synthetic values under AOT, because there
/// is no cache to accumulate into. Comparing them to FPM would measure the fidelity of a number
/// the model deliberately invents.
///
/// What IS a real defect, and what this pins, is the surface changing shape between requests of
/// one worker: a key appearing or vanishing, or iteration order drifting. Nothing in a
/// single-request CLI test can observe that.
///
/// The fingerprint is built with `foreach` and no sorting on purpose — elephc's checker refuses
/// `ksort()`/`sort()` on the `Mixed` arrays these functions return, and comparing raw iteration
/// order across requests is a STRICTER check than comparing sorted sets anyway.
#[test]
fn web_opcache_surface_keeps_one_shape_across_requests() {
    let dir = make_test_dir("web_opcache_shape");
    let src = "<?php \
$s = opcache_get_status(); \
foreach ($s as $k => $v) { echo 'S', $k, '|'; } \
foreach ($s['opcache_statistics'] as $k => $v) { echo 'T', $k, '|'; } \
foreach ($s['scripts'] as $p => $e) { foreach ($e as $k => $v) { echo 'E', $k, '|'; } } \
$c = opcache_get_configuration(); \
foreach ($c['directives'] as $k => $v) { echo 'D', $k, '|'; }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let second = http_get(&addr, "/");
    let third = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    let body = |r: &str| r.rsplit("\r\n\r\n").next().unwrap_or("").to_string();
    assert!(!body(&first).is_empty(), "the first request produced no body: {first:?}");
    assert_eq!(
        body(&first),
        body(&second),
        "the OPcache surface changed shape between request 1 and 2"
    );
    assert_eq!(
        body(&second),
        body(&third),
        "the OPcache surface changed shape between request 2 and 3"
    );
}

/// Verifies the reporting-only counters stay COHERENT across requests of one worker.
///
/// `start_time` must not move — it identifies the worker's cache generation, and a value that
/// drifted per request would make every rate derived from it meaningless. `num_cached_scripts`
/// must not shrink: the AOT manifest is fixed at link time, so an entry disappearing would mean
/// the model lost track of code that is still in the binary.
///
/// Both are class-B invariants: the NUMBERS are synthetic, their COHERENCE is not.
#[test]
fn web_opcache_counters_stay_coherent_across_requests() {
    let dir = make_test_dir("web_opcache_counters");
    let src = "<?php \
$s = opcache_get_status(); \
echo $s['opcache_statistics']['start_time'], ':', \
     $s['opcache_statistics']['num_cached_scripts'], ':', \
     ($s['opcache_statistics']['opcache_hit_rate'] >= 0 ? 'HR_OK' : 'HR_NEG');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let second = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    let body = |r: &str| r.rsplit("\r\n\r\n").next().unwrap_or("").to_string();
    let (a, b) = (body(&first), body(&second));
    assert!(a.ends_with("HR_OK"), "hit rate must never be negative: {a:?}");
    assert_eq!(
        a, b,
        "start_time / num_cached_scripts must not drift between requests"
    );
}

/// Verifies the request-global default stream context is recreated as a live
/// resource after each request reset rather than reusing a stale registry handle.
#[test]
fn web_default_stream_context_is_live_on_every_request() {
    let dir = make_test_dir("web_default_stream_context_reset");
    let src = "<?php $context = stream_context_get_default(); \
        echo is_resource($context) ? get_resource_type($context) : 'dead';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let second = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        first.ends_with("stream-context"),
        "first request returned a non-live default context: {:?}",
        first
    );
    assert!(
        second.ends_with("stream-context"),
        "request reset left a stale default-context handle: {:?}",
        second
    );
}

/// Verifies `opcache.enable_cli` has NO effect under `--web`, where only `opcache.enable` governs.
///
/// php-src consults `enable_cli` solely on the CLI SAPI; a web request reads `opcache.enable`
/// alone. elephc resolves that gate at COMPILE time from the target SAPI, so a `--web` build with
/// `enable_cli=0` must still report an enabled cache — and a `--web` build with `enable=0` must
/// report it disabled even if `enable_cli=1`. Getting this backwards would produce a binary that
/// contradicts its own `opcache_get_configuration()`.
///
/// This is a class-A contract check and needs no reference process, which is exactly why it
/// belongs here rather than in an FPM comparison.
#[test]
fn web_opcache_gate_ignores_enable_cli() {
    let src = "<?php $s = opcache_get_status(); echo is_array($s) ? 'ON' : 'OFF';";

    for (flags, expected, why) in [
        (
            vec!["--ini", "opcache.enable=1", "--ini", "opcache.enable_cli=0"],
            "ON",
            "enable_cli=0 must not disable a web build",
        ),
        (
            vec!["--ini", "opcache.enable=0", "--ini", "opcache.enable_cli=1"],
            "OFF",
            "enable_cli=1 must not enable a web build whose opcache.enable is 0",
        ),
    ] {
        let dir = make_test_dir("web_opcache_gate");
        let bin = compile_web_with_flags(&dir, src, "app", &flags);
        let port = free_port();
        let addr = format!("127.0.0.1:{}", port);
        let mut child = spawn_server(&bin, &addr, "1");
        let resp = http_get(&addr, "/");
        let _ = child.kill();
        let _ = child.wait();

        assert!(resp.ends_with(expected), "{why}; response was {resp:?}");
    }
}
