//! Purpose:
//! End-to-end web tests for PHP session support under `--web`.
//!
//! Called from:
//! - `cargo test --test web_session_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests compile PHP with `--web`, spawn the server, drive HTTP requests,
//!   and assert response bodies and headers.
//! - Session persistence tests extract the Set-Cookie header from the first
//!   response and send it back as a Cookie header on the second request.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Compiles `source` with `--web`; returns the binary path.
fn compile_web(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--web").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc --web failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
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

/// Spawns the server binary on `addr`, waits until it accepts connections.
fn spawn_server(bin: &Path, addr: &str, workers: &str) -> std::process::Child {
    let child = Command::new(bin)
        .arg("--listen").arg(addr)
        .arg("--workers").arg(workers)
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// Sends one HTTP/1.1 GET and returns the full raw response text.
fn http_get(addr: &str, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", path, addr);
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    buf
}

/// Sends one HTTP/1.1 request with a method, custom headers, and body; returns the full raw response.
fn http_request(addr: &str, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, addr);
    for (k, v) in headers {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    buf
}

/// Extracts the `PHPSESSID=...` value from a Set-Cookie header in the raw response.
fn extract_phpsessid(resp: &str) -> Option<String> {
    for line in resp.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("set-cookie:") && lower.contains("phpsessid=") {
            let rest = &line["set-cookie:".len()..];
            let rest = rest.trim();
            if let Some(eq) = rest.find("PHPSESSID=") {
                let after = &rest[eq + "PHPSESSID=".len()..];
                let end = after.find(|c: char| c == ';' || c == ' ' || c == '\r').unwrap_or(after.len());
                return Some(after[..end].to_string());
            }
            if let Some(eq) = rest.find("phpsessid=") {
                let after = &rest[eq + "phpsessid=".len()..];
                let end = after.find(|c: char| c == ';' || c == ' ' || c == '\r').unwrap_or(after.len());
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

/// Verifies that `session_start()` activates a session and `session_status()`
/// returns `PHP_SESSION_ACTIVE` (2) under `--web`.
#[test]
fn session_start_basic() {
    let dir = make_test_dir("sess_start");
    let bin = compile_web(&dir, "<?php session_start(); echo session_status();", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("2"), "session_status should be 2 (ACTIVE): {:?}", resp);
}

/// Verifies that `session_id()` returns a non-empty 32-char hex string after
/// `session_start()`.
#[test]
fn session_id_not_empty() {
    let dir = make_test_dir("sess_id");
    let bin = compile_web(&dir, "<?php session_start(); echo strlen(session_id());", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("32"), "session_id length should be 32: {:?}", resp);
}

/// Verifies that `session_name()` returns the default name `PHPSESSID`.
#[test]
fn session_name_default() {
    let dir = make_test_dir("sess_name");
    let bin = compile_web(&dir, "<?php session_start(); echo session_name();", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.contains("PHPSESSID"), "session_name should contain PHPSESSID: {:?}", resp);
}

/// Verifies that `session_start()` causes a `Set-Cookie: PHPSESSID=...` header
/// to be sent in the HTTP response.
#[test]
fn session_cookie_sent() {
    let dir = make_test_dir("sess_cookie");
    let bin = compile_web(&dir, "<?php session_start(); echo \"ok\";", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.to_lowercase().contains("set-cookie: phpsessid="),
        "response should contain Set-Cookie: PHPSESSID= header: {:?}",
        resp
    );
}

/// Verifies session persistence across two HTTP requests: a counter stored in
/// `$_SESSION` increments from 1 to 2 when the same session cookie is sent back.
#[test]
fn session_counter_persists() {
    let dir = make_test_dir("sess_persist");
    let src = "<?php session_start(); if (!isset($_SESSION['hits'])) { $_SESSION['hits'] = 0; } $_SESSION['hits'] = $_SESSION['hits'] + 1; echo $_SESSION['hits'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let cookie = extract_phpsessid(&r1).expect("first response should set a PHPSESSID cookie");
    let r2 = http_request(&addr, "GET", "/", &[("Cookie", &format!("PHPSESSID={}", cookie))], "");
    let _ = child.kill();
    let _ = child.wait();
    assert!(r1.ends_with("1"), "first request counter should be 1: {:?}", r1);
    assert!(r2.ends_with("2"), "second request counter should be 2: {:?}", r2);
}

/// Verifies that `session_unset()` clears all `$_SESSION` variables so a
/// previously-set key is no longer isset.
#[test]
fn session_unset_clears() {
    let dir = make_test_dir("sess_unset");
    let src = "<?php session_start(); $_SESSION['x'] = 'val'; session_unset(); echo isset($_SESSION['x']) ? 'yes' : 'no';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("no"), "session_unset should clear $_SESSION: {:?}", resp);
}

/// Verifies that `session_destroy()` ends the session so `session_status()`
/// returns `PHP_SESSION_NONE` (1).
#[test]
fn session_destroy_clears() {
    let dir = make_test_dir("sess_destroy");
    let src = "<?php session_start(); $_SESSION['x'] = 'val'; session_destroy(); echo session_status();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("1"), "session_status after destroy should be 1 (NONE): {:?}", resp);
}

/// Verifies that the `PHP_SESSION_DISABLED`, `PHP_SESSION_NONE`, and
/// `PHP_SESSION_ACTIVE` constants have values 0, 1, and 2 respectively.
#[test]
fn session_constants() {
    let dir = make_test_dir("sess_const");
    let bin = compile_web(
        &dir,
        "<?php echo PHP_SESSION_DISABLED . PHP_SESSION_NONE . PHP_SESSION_ACTIVE;",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("012"), "session constants should concatenate to 012: {:?}", resp);
}

/// Verifies that `session_encode()` produces PHP serialize-format output
/// containing the key prefix and serialized string value.
#[test]
fn session_encode_decode() {
    let dir = make_test_dir("sess_encode");
    let bin = compile_web(
        &dir,
        "<?php session_start(); $_SESSION['k'] = 'v'; $enc = session_encode(); echo $enc;",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.contains("k|"), "encoded session should contain 'k|': {:?}", resp);
    assert!(
        resp.contains("s:1:\"v\""),
        "encoded session should contain serialized string s:1:\"v\": {:?}",
        resp
    );
}

/// Verifies that `session_write_close()` closes the session so
/// `session_status()` returns `PHP_SESSION_NONE` (1).
#[test]
fn session_write_close_status() {
    let dir = make_test_dir("sess_write_close");
    let bin = compile_web(
        &dir,
        "<?php session_start(); session_write_close(); echo session_status();",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("1"),
        "session_status after write_close should be 1 (NONE): {:?}",
        resp
    );
}

/// Verifies that `session_save_path()` returns a non-empty path (the default
/// session save directory).
#[test]
fn session_save_path() {
    let dir = make_test_dir("sess_savepath");
    let bin = compile_web(
        &dir,
        "<?php echo strlen(session_save_path()) > 0 ? 'yes' : 'no';",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("yes"), "session_save_path should be non-empty: {:?}", resp);
}

/// Verifies that `session_module_name()` returns `files` (the default files
/// session handler module).
#[test]
fn session_module_name() {
    let dir = make_test_dir("sess_module");
    let bin = compile_web(&dir, "<?php echo session_module_name();", "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(resp.ends_with("files"), "session_module_name should be 'files': {:?}", resp);
}
