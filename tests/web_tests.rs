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

/// Compiles `source` with the given extra elephc flags; returns the binary path.
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

/// Spawns the server binary on `addr`, waits until it accepts connections.
fn spawn_server(bin: &Path, addr: &str, workers: &str) -> std::process::Child {
    let child = Command::new(bin)
        .arg("--listen").arg(addr)
        .arg("--workers").arg(workers)
        .spawn()
        .expect("failed to spawn web server");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return child;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {}", addr);
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

/// Verifies a trivial program compiles under --web and produces an executable file.
#[test]
fn web_compile_produces_binary() {
    let dir = make_test_dir("web_compile");
    let bin = compile_web(&dir, "<?php echo \"Hello World\";", "app");
    assert!(bin.exists(), "expected binary at {}", bin.display());
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
    assert!(r1.ends_with("x"), "first response body: {:?}", r1);
    assert!(r2.ends_with("x"), "second response body: {:?}", r2);
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
