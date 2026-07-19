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
    compile_web_with_flags(dir, source, stem, &[])
}

/// Compiles `source` with `--web` and additional compiler flags.
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
        .arg("--listen")
        .arg(addr)
        .arg("--workers")
        .arg(workers)
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// Spawns the server binary with extra environment variables set on the server
/// process, waiting until it accepts connections. Used to exercise process-level
/// config (e.g. `ELEPHC_SESSION_AUTO_START`) that the workers read at startup.
fn spawn_server_with_env(
    bin: &Path,
    addr: &str,
    workers: &str,
    env: &[(&str, &str)],
) -> std::process::Child {
    let mut cmd = Command::new(bin);
    cmd.arg("--listen").arg(addr).arg("--workers").arg(workers);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// Spawns the server with stderr redirected to a file for diagnostic assertions.
fn spawn_server_with_stderr(bin: &Path, addr: &str, stderr_path: &Path) -> std::process::Child {
    let stderr = fs::File::create(stderr_path).expect("failed to create server stderr capture");
    let child = Command::new(bin)
        .arg("--listen")
        .arg(addr)
        .arg("--workers")
        .arg("1")
        .stderr(stderr)
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// Sends one HTTP/1.1 GET and returns the full raw response text.
fn http_get(addr: &str, path: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    s.write_all(req.as_bytes()).unwrap();
    read_response(&mut s)
}

/// Sends one HTTP/1.1 request with a method, custom headers, and body; returns the full raw response.
fn http_request(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
        method, path, addr
    );
    for (k, v) in headers {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{}", body.len(), body));
    s.write_all(req.as_bytes()).unwrap();
    read_response(&mut s)
}

/// Reads the HTTP response from `s` and returns the raw bytes as a string.
/// Stops once the server has sent the full response: either it closes the
/// connection (EOF), a read timeout fires (no more data — keep-alive server),
/// or the body has been fully received (Content-Length or the connection is
/// dropped). A read timeout is treated as "no more data" rather than an error,
/// because the elephc web server keeps connections open in keep-alive mode.
fn read_response(s: &mut TcpStream) -> String {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if response_complete(&buf) {
                    break;
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => panic!("read error: {e}"),
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Returns true once the HTTP response in `buf` is complete: headers + body
/// matching the Content-Length, or a chunked terminator, or a 1xx/204/304
/// status with no body.
fn response_complete(buf: &[u8]) -> bool {
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let hsep = match text.find("\r\n\r\n") {
        Some(i) => i,
        None => return false,
    };
    let headers = &text[..hsep];
    let body = &text[hsep + 4..];
    let status_line = headers.lines().next().unwrap_or("");
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if (100..200).contains(&status_code) || status_code == 204 || status_code == 304 {
        return true;
    }
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in headers.lines().skip(1) {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            content_length = rest.trim().parse().ok();
        }
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    if chunked {
        return body.ends_with("0\r\n\r\n");
    }
    if let Some(len) = content_length {
        return body.len() >= len;
    }
    false
}

/// Extracts one named session-cookie value from a raw HTTP response.
fn extract_session_id(resp: &str, name: &str) -> Option<String> {
    let needle = format!("{}=", name);
    let lower_needle = needle.to_ascii_lowercase();
    for line in resp.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("set-cookie:") && lower.contains(&lower_needle) {
            let rest = &line["set-cookie:".len()..];
            let rest = rest.trim();
            if let Some(eq) = rest.to_ascii_lowercase().find(&lower_needle) {
                let after = &rest[eq + needle.len()..];
                let end = after
                    .find(|c: char| c == ';' || c == ' ' || c == '\r')
                    .unwrap_or(after.len());
                return Some(after[..end].to_string());
            }
        }
    }
    None
}

/// Extracts the default `PHPSESSID=...` value from a raw response.
fn extract_phpsessid(resp: &str) -> Option<String> {
    extract_session_id(resp, "PHPSESSID")
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
    assert!(
        resp.ends_with("2"),
        "session_status should be 2 (ACTIVE): {:?}",
        resp
    );
}

/// Verifies one maintained PHP session profile against the upstream version
/// boundaries for CHIPS, defaults, long prefixes, and option-map keys.
fn assert_maintained_php_session_profile(version: &str, expected: &str) {
    let source = r#"<?php
$params = session_get_cookie_params();
echo count($params) . ':';
echo ini_get('session.cookie_partitioned') === false ? 'absent:' : 'present:';
echo ini_get('session.cookie_httponly') . ':' . ini_get('session.use_strict_mode') . ':';
try {
    echo strlen(session_create_id(str_repeat('a', 257)));
} catch (ValueError $e) {
    echo 'ValueError';
}
echo ':';
try {
    session_start(['read_and_close' => 'false']);
    echo 'accepted';
    session_write_close();
} catch (TypeError $e) {
    echo 'TypeError';
}
echo ':';
try {
    session_start([0 => 'false']);
    echo 'started';
} catch (ValueError $e) {
    echo 'ValueError';
}
"#;
    let dir = make_test_dir(&format!("sess_php_{}", version.replace('.', "_")));
    let version_flag = format!("--php-version={version}");
    let bin = compile_web_with_flags(&dir, source, "app", &[&version_flag]);
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        response.ends_with(expected),
        "PHP {version} session profile mismatch: {response:?}"
    );
}

/// Verifies the maintained PHP 8.2 session profile.
#[test]
fn maintained_php_82_session_profile() {
    assert_maintained_php_session_profile("8.2", "6:absent:::289:accepted:started");
}

/// Verifies the maintained PHP 8.3 session profile.
#[test]
fn maintained_php_83_session_profile() {
    assert_maintained_php_session_profile("8.3", "6:absent:::289:accepted:started");
}

/// Verifies the maintained PHP 8.4 session profile.
#[test]
fn maintained_php_84_session_profile() {
    assert_maintained_php_session_profile("8.4", "6:absent:::ValueError:accepted:started");
}

/// Verifies the maintained PHP 8.5 session profile.
#[test]
fn maintained_php_85_session_profile() {
    assert_maintained_php_session_profile("8.5", "7:present:::ValueError:TypeError:ValueError");
}

/// Verifies PHP 8.4 starts emitting the upstream trans-SID deprecation while
/// PHP 8.3 accepts the same runtime setting without a deprecation.
#[test]
fn php84_session_deprecations_are_version_gated() {
    let source = "<?php ini_set('session.use_trans_sid', '1'); echo 'ok';";
    for (version, deprecated) in [("8.3", false), ("8.4", true)] {
        let dir = make_test_dir(&format!("sess_deprecation_{}", version.replace('.', "_")));
        let version_flag = format!("--php-version={version}");
        let bin = compile_web_with_flags(&dir, source, "app", &[&version_flag]);
        let stderr_path = dir.join("server.stderr");
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let mut child = spawn_server_with_stderr(&bin, &addr, &stderr_path);
        let response = http_get(&addr, "/");
        let _ = child.kill();
        let _ = child.wait();
        let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
        assert!(
            response.ends_with("ok"),
            "PHP {version} response: {response:?}"
        );
        assert_eq!(
            stderr.contains(
                "Deprecated: ini_set(): Enabling session.use_trans_sid INI setting is deprecated"
            ),
            deprecated,
            "PHP {version} stderr mismatch: {stderr:?}"
        );
    }
}

/// Verifies PHP 8.5 adds the upstream warning for `|` in a `php` serializer
/// key while preserving PHP 8.4's silent `false` result.
#[test]
fn php85_invalid_session_key_warning_is_version_gated() {
    let source = "<?php session_start(); $_SESSION['bad|key'] = 1; echo session_encode() === false ? 'false' : 'encoded'; session_abort();";
    for (version, warns) in [("8.4", false), ("8.5", true)] {
        let dir = make_test_dir(&format!("sess_pipe_warning_{}", version.replace('.', "_")));
        let version_flag = format!("--php-version={version}");
        let bin = compile_web_with_flags(&dir, source, "app", &[&version_flag]);
        let stderr_path = dir.join("server.stderr");
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        let mut child = spawn_server_with_stderr(&bin, &addr, &stderr_path);
        let response = http_get(&addr, "/");
        let _ = child.kill();
        let _ = child.wait();
        let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
        assert!(
            response.ends_with("false"),
            "PHP {version} response: {response:?}"
        );
        assert_eq!(
            stderr.contains(
                "Warning: session_encode(): Failed to write session data. Data contains invalid key \"bad|key\""
            ),
            warns,
            "PHP {version} stderr mismatch: {stderr:?}"
        );
    }
}

/// Regression test: the web prelude initializes `$_SESSION` so that the
/// `finally` block can call `session_write_close()` even when the user handler
/// never touches `$_SESSION` directly.
#[test]
fn session_finalize_without_user_touch() {
    let dir = make_test_dir("sess_finalize");
    let bin = compile_web(
        &dir,
        "<?php elephc_web_session_set_status(2); echo 'ok';",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("ok"),
        "response should end with 'ok': {:?}",
        resp
    );
}

/// Verifies that `session_id()` returns a non-empty 32-char hex string after
/// `session_start()`.
#[test]
fn session_id_not_empty() {
    let dir = make_test_dir("sess_id");
    let bin = compile_web(
        &dir,
        "<?php session_start(); echo strlen(session_id());",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("32"),
        "session_id length should be 32: {:?}",
        resp
    );
}

/// Regression: `session.sid_length` sizes only the random suffix, so the files
/// handler must accept a complete ID returned by `session_create_id($prefix)`.
#[test]
fn session_create_id_prefix_is_accepted_by_files_handler() {
    let dir = make_test_dir("sess_prefixed_id");
    let save_path = dir.to_string_lossy().replace('\\', "/");
    let source = format!(
        "<?php\n\
         $id = (string) session_create_id('abc');\n\
         session_id($id);\n\
         $started = session_start(['save_path' => '{save_path}', 'use_cookies' => false]);\n\
         echo strlen($id) . ':' . ($started ? 'started' : 'failed');\n\
         session_abort();"
    );
    let bin = compile_web(&dir, &source, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        response.ends_with("35:started"),
        "prefix plus the default 32-byte suffix must be accepted: {response:?}"
    );
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
    assert!(
        resp.contains("PHPSESSID"),
        "session_name should contain PHPSESSID: {:?}",
        resp
    );
}

/// Verifies `session_name()` rejects CRLF and the rest of PHP's forbidden
/// `session.name` characters before the value can reach a `Set-Cookie` header.
#[test]
fn session_name_rejects_header_injection_characters() {
    let dir = make_test_dir("sess_name_injection");
    let source = r#"<?php
$old = session_name("Bad\r\nX-Elephc-Injected: yes");
$current = session_name();
session_start();
echo $old . '|' . $current;
"#;
    let bin = compile_web(&dir, source, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        response
            .to_ascii_lowercase()
            .contains("set-cookie: phpsessid="),
        "the valid default name should still own the cookie: {response:?}"
    );
    assert!(
        !response.to_ascii_lowercase().contains("x-elephc-injected:"),
        "invalid name escaped into response headers: {response:?}"
    );
    assert!(
        response.ends_with("PHPSESSID|PHPSESSID"),
        "invalid session_name must return and preserve the old name: {response:?}"
    );
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
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", cookie))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("1"),
        "first request counter should be 1: {:?}",
        r1
    );
    assert!(
        r2.ends_with("2"),
        "second request counter should be 2: {:?}",
        r2
    );
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
    assert!(
        resp.ends_with("no"),
        "session_unset should clear $_SESSION: {:?}",
        resp
    );
}

/// Verifies that `session_destroy()` ends the session so `session_status()`
/// returns `PHP_SESSION_NONE` (1).
#[test]
fn session_destroy_clears() {
    let dir = make_test_dir("sess_destroy");
    let src =
        "<?php session_start(); $_SESSION['x'] = 'val'; session_destroy(); echo session_status();";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("1"),
        "session_status after destroy should be 1 (NONE): {:?}",
        resp
    );
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
    assert!(
        resp.ends_with("012"),
        "session constants should concatenate to 012: {:?}",
        resp
    );
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
    assert!(
        resp.contains("k|"),
        "encoded session should contain 'k|': {:?}",
        resp
    );
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

/// Regression: an unencodable `php`-serializer key must write an empty payload
/// and release the files-handler lock, allowing the same request to reopen the SID.
#[test]
fn session_write_close_encode_failure_releases_lock() {
    let dir = make_test_dir("sess_encode_close_unlock");
    let save_path = dir.to_string_lossy().replace('\\', "/");
    let sid = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let source = format!(
        "<?php\n\
         session_id('{sid}');\n\
         session_start(['save_path' => '{save_path}', 'use_cookies' => false]);\n\
         $_SESSION['bad|key'] = 1;\n\
         echo session_write_close() ? 'closed:' : 'close-failed:';\n\
         echo session_start(['save_path' => '{save_path}', 'use_cookies' => false]) ? 'reopened:' : 'reopen-failed:';\n\
         echo count($_SESSION);\n\
         session_abort();"
    );
    let bin = compile_web(&dir, &source, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        response.ends_with("closed:reopened:0"),
        "encode failure must close and reopen without deadlock: {response:?}"
    );
    assert_eq!(
        fs::metadata(dir.join(format!("sess_{sid}"))).unwrap().len(),
        0,
        "php-src writes an empty session payload after encode failure"
    );
}

/// Regression: the stock `SessionHandler` object delegates `close()` to the
/// files bridge so `session_abort()` cannot leave its read-time flock held.
#[test]
fn stock_session_handler_close_releases_lock() {
    let dir = make_test_dir("sess_handler_close_unlock");
    let save_path = dir.to_string_lossy().replace('\\', "/");
    let source = format!(
        "<?php\n\
         $handler = new SessionHandler();\n\
         session_set_save_handler($handler, true);\n\
         session_id('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');\n\
         session_start(['save_path' => '{save_path}', 'use_cookies' => false]);\n\
         $_SESSION['discarded'] = 1;\n\
         session_abort();\n\
         echo session_start(['save_path' => '{save_path}', 'use_cookies' => false]) ? 'reopened' : 'failed';\n\
         session_abort();"
    );
    let bin = compile_web(&dir, &source, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        response.ends_with("reopened"),
        "SessionHandler::close must release the file lock: {response:?}"
    );
}

/// Verifies php-src's empty configured save-path default. The files bridge
/// resolves it to the platform temporary directory only when opening storage.
#[test]
fn session_save_path() {
    let dir = make_test_dir("sess_savepath");
    let bin = compile_web(
        &dir,
        "<?php echo session_save_path() === '' ? 'empty' : 'configured';",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("empty"),
        "session_save_path should expose the empty configured default: {:?}",
        resp
    );
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
    assert!(
        resp.ends_with("files"),
        "session_module_name should be 'files': {:?}",
        resp
    );
}

/// Verifies a custom `SessionHandlerInterface` registered via
/// `session_set_save_handler()` is actually dispatched: `session_module_name()`
/// reports `user`, and the handler's `read`/`write` persist `$_SESSION` across two
/// requests (a counter increments 1 → 2) using the handler's own file storage —
/// not the built-in files bridge. This is the end-to-end regression guard for the
/// EIR fix that lets an object stored in a static property dispatch after being
/// borrowed across a function boundary (`lower_static_property_assign` acquire).
#[test]
fn session_custom_save_handler_round_trip() {
    let dir = make_test_dir("sess_handler");
    // The handler stores each session under `<dir>/h_<id>`, so persistence flows
    // through the user handler rather than the default `sess_<id>` bridge files.
    let store = dir.join("store");
    fs::create_dir_all(&store).unwrap();
    let store_str = store.to_string_lossy().replace('\\', "/");
    let src = format!(
        "<?php\n\
        class FileHandler implements SessionHandlerInterface {{\n\
            public function open(string $p, string $n): bool {{ return true; }}\n\
            public function close(): bool {{ return true; }}\n\
            public function read(string $id): string|false {{\n\
                $f = '{store}/h_' . $id;\n\
                if (!file_exists($f)) {{ return ''; }}\n\
                return file_get_contents($f);\n\
            }}\n\
            public function write(string $id, string $data): bool {{\n\
                file_put_contents('{store}/h_' . $id, $data); return true;\n\
            }}\n\
            public function destroy(string $id): bool {{ return true; }}\n\
            public function gc(int $m): int|false {{ return 0; }}\n\
        }}\n\
        session_set_save_handler(new FileHandler());\n\
        header('Content-Type: text/plain');\n\
        echo session_module_name() . ':';\n\
        session_start();\n\
        if (!isset($_SESSION['hits'])) {{ $_SESSION['hits'] = 0; }}\n\
        $_SESSION['hits'] = $_SESSION['hits'] + 1;\n\
        echo $_SESSION['hits'];\n",
        store = store_str
    );
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let cookie = extract_phpsessid(&r1).expect("handler run should set a PHPSESSID cookie");
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", cookie))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("user:1"),
        "first request should report the user handler and counter 1: {:?}",
        r1
    );
    assert!(
        r2.ends_with("user:2"),
        "second request should persist via the handler to counter 2: {:?}",
        r2
    );
    // The handler's own storage file must exist; the default bridge file must not.
    assert!(
        store.join(format!("h_{}", cookie)).exists(),
        "custom handler should have written its own store file"
    );
    let default_file = std::env::temp_dir().join(format!("sess_{}", cookie));
    assert!(
        !default_file.exists(),
        "default bridge file must not be written when a handler is registered"
    );
}

/// End-to-end test for the legacy 6-callable form of `session_set_save_handler`
/// (deprecated in PHP 8.4). Registers six plain function-name callables that store
/// each session under `<dir>/store/c_<id>`, then verifies `session_module_name()`
/// reports `user` and a `$_SESSION` counter persists across two requests (1 → 2)
/// through those callables — proving the internal callable adapter dispatches
/// open/read/write/close via `call_user_func` and persists to the handler's own
/// storage, not the built-in bridge.
#[test]
fn session_callable_save_handler_round_trip() {
    let dir = make_test_dir("sess_cb_handler");
    let store = dir.join("store");
    fs::create_dir_all(&store).unwrap();
    let store_str = store.to_string_lossy().replace('\\', "/");
    let src = format!(
        "<?php\n\
        function h_open(string $p, string $n): bool {{ return true; }}\n\
        function h_close(): bool {{ return true; }}\n\
        function h_read(string $id): string|false {{\n\
            $f = '{store}/c_' . $id;\n\
            if (!file_exists($f)) {{ return ''; }}\n\
            return file_get_contents($f);\n\
        }}\n\
        function h_write(string $id, string $data): bool {{\n\
            file_put_contents('{store}/c_' . $id, $data); return true;\n\
        }}\n\
        function h_destroy(string $id): bool {{ return true; }}\n\
        function h_gc(int $m): int {{ return 0; }}\n\
        session_set_save_handler('h_open', 'h_close', 'h_read', 'h_write', 'h_destroy', 'h_gc');\n\
        header('Content-Type: text/plain');\n\
        echo session_module_name() . ':';\n\
        session_start();\n\
        if (!isset($_SESSION['hits'])) {{ $_SESSION['hits'] = 0; }}\n\
        $_SESSION['hits'] = $_SESSION['hits'] + 1;\n\
        echo $_SESSION['hits'];\n",
        store = store_str
    );
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let cookie =
        extract_phpsessid(&r1).expect("callable handler run should set a PHPSESSID cookie");
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", cookie))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("user:1"),
        "first request should report the callable handler and counter 1: {:?}",
        r1
    );
    assert!(
        r2.ends_with("user:2"),
        "second request should persist via the callables to counter 2: {:?}",
        r2
    );
    assert!(
        store.join(format!("c_{}", cookie)).exists(),
        "callable handler should have written its own store file"
    );
    let default_file = std::env::temp_dir().join(format!("sess_{}", cookie));
    assert!(
        !default_file.exists(),
        "default bridge file must not be written when a callable handler is registered"
    );
}

/// Verifies `session_abort()` leaves current in-memory values intact but does not
/// persist them. Each later request reads `orig`, changes it, aborts, and observes
/// `changed`; repeating that sequence proves the stored value remained `orig`.
#[test]
fn session_abort_preserves_memory_and_discards_persistence() {
    let dir = make_test_dir("sess_abort");
    let src = "<?php session_start(); if (!isset($_SESSION['v'])) { $_SESSION['v'] = 'orig'; echo 'set:' . $_SESSION['v']; } else { $_SESSION['v'] = 'changed'; session_abort(); echo 'after:' . $_SESSION['v']; }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let r1 = http_get(&addr, "/");
    let cookie = extract_phpsessid(&r1).expect("first response should set a PHPSESSID cookie");
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", cookie))],
        "",
    );
    let r3 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", cookie))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("set:orig"),
        "first request should seed v=orig: {:?}",
        r1
    );
    assert!(
        r2.ends_with("after:changed"),
        "session_abort must leave the current in-memory value intact: {:?}",
        r2
    );
    assert!(
        r3.ends_with("after:changed"),
        "the stored value must still be orig before the third request changes it again: {:?}",
        r3
    );
}

/// Verifies the `ini_set`/`ini_get` round-trip on a session directive:
/// `ini_set('session.gc_maxlifetime', 999)` returns the old value ("1440") and a
/// subsequent `ini_get('session.gc_maxlifetime')` reflects the new value ("999").
#[test]
fn ini_set_get_round_trip() {
    let dir = make_test_dir("ini_roundtrip");
    let src = "<?php $old = ini_set('session.gc_maxlifetime', 999); echo $old . '|' . ini_get('session.gc_maxlifetime');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("1440|999"),
        "ini_set should return old 1440 and ini_get the new 999: {:?}",
        resp
    );
}

/// Verifies `ini_get('session.name')` reflects the session name: it returns the
/// default `PHPSESSID` initially, and after `session_name('X')` it returns `X`
/// (proving `ini_get`/`ini_set` share the same bridge state as the `session_*`
/// setters).
#[test]
fn ini_get_session_name_tracks_session_name() {
    let dir = make_test_dir("ini_name");
    let src = "<?php echo ini_get('session.name'); session_name('X'); echo '|' . ini_get('session.name');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("PHPSESSID|X"),
        "ini_get('session.name') should be PHPSESSID then X: {:?}",
        resp
    );
}

/// Verifies `session.auto_start`: with `ELEPHC_SESSION_AUTO_START=1` set on the
/// server process, a session is active (`session_status()` is `PHP_SESSION_ACTIVE`
/// = 2) even though the handler never calls `session_start()` — the prelude
/// bootstrap auto-starts it. `ini_get('session.auto_start')` also reports "1".
#[test]
fn session_auto_start_env_activates_session() {
    let dir = make_test_dir("auto_start");
    let src = "<?php echo session_status() . '|' . ini_get('session.auto_start');";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server_with_env(&bin, &addr, "1", &[("ELEPHC_SESSION_AUTO_START", "1")]);
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("2|1"),
        "auto_start should make the session ACTIVE (2) without an explicit session_start(): {:?}",
        resp
    );
}

/// Verifies `session.referer_check` invalidates a cookie-supplied ID whose
/// request `Referer` does not contain the configured substring. PHP applies
/// this legacy URL-session check only when `session.use_only_cookies=0`, so the
/// fixture opts into that mode while still carrying the ID through its cookie.
/// Three requests share one cookie: (1) matching Referer seeds `v=x`; (2) a
/// matching Referer sees the persisted `has:x`; (3) a NON-matching Referer is
/// rejected, so the session starts fresh and re-seeds (`set:x`).
#[test]
fn session_referer_check_rejects_mismatched_referer() {
    let dir = make_test_dir("referer_check");
    let src = "<?php session_start(['referer_check' => 'example.com', 'use_only_cookies' => 0]); if (isset($_SESSION['v'])) { echo 'has:' . $_SESSION['v']; } else { $_SESSION['v'] = 'x'; echo 'set:x'; }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    // Request 1: no cookie yet, matching Referer → seeds v=x and sets the cookie.
    let r1 = http_request(
        &addr,
        "GET",
        "/",
        &[("Referer", "http://example.com/page")],
        "",
    );
    let cookie = extract_phpsessid(&r1).expect("first response should set a PHPSESSID cookie");
    let cookie_hdr = format!("PHPSESSID={}", cookie);
    // Request 2: same cookie, matching Referer → sees the persisted value.
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("Cookie", &cookie_hdr),
            ("Referer", "http://example.com/other"),
        ],
        "",
    );
    // Request 3: same cookie, NON-matching Referer → id rejected, fresh session.
    let r3 = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("Cookie", &cookie_hdr),
            ("Referer", "http://evil.example.org/"),
        ],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        r1.ends_with("set:x"),
        "first request should seed v=x: {:?}",
        r1
    );
    assert!(
        r2.ends_with("has:x"),
        "matching Referer should see the persisted session: {:?}",
        r2
    );
    assert!(
        r3.ends_with("set:x"),
        "non-matching Referer must start a fresh session (referer_check): {:?}",
        r3
    );
}

/// Spawns the server with its stderr redirected to `stderr_file` so tests can
/// read the worker's E_WARNING/E_NOTICE channel back after the request runs.
/// Workers inherit the redirected fd, so `trigger_error()`/`error_log()` output
/// (written unbuffered via the runtime `fwrite`) lands in the file.
fn spawn_server_stderr_to_file(
    bin: &Path,
    addr: &str,
    workers: &str,
    stderr_file: &Path,
) -> std::process::Child {
    let f = fs::File::create(stderr_file).unwrap();
    let child = Command::new(bin)
        .arg("--listen")
        .arg(addr)
        .arg("--workers")
        .arg(workers)
        .stderr(std::process::Stdio::from(f))
        .spawn()
        .expect("failed to spawn web server");
    wait_until_ready(addr);
    child
}

/// Verifies `error_log($msg, 3, $file)` appends each message verbatim to the
/// destination file and returns true. The file is the clean, deterministic proof
/// of the error_log path (separate from the HTTP response channel).
#[test]
fn error_log_type3_appends_to_file() {
    let dir = make_test_dir("errlog3");
    let logpath = dir.join("app.log");
    let logstr = logpath.to_string_lossy().replace('\\', "\\\\");
    let src = format!(
        "<?php $ok = error_log(\"line-one\\n\", 3, \"{p}\"); error_log(\"line-two\\n\", 3, \"{p}\"); echo $ok ? 'T' : 'F';",
        p = logstr
    );
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("T"),
        "error_log(type 3) should return true: {:?}",
        resp
    );
    let contents = fs::read_to_string(&logpath).unwrap_or_default();
    assert_eq!(
        contents, "line-one\nline-two\n",
        "error_log(type 3) should append both messages verbatim: {:?}",
        contents
    );
}

/// Verifies `error_log($msg, 3, $bad)` returns false when the destination cannot
/// be opened, and true for a writable destination — proving the open-failure path.
#[test]
fn error_log_type3_open_failure_returns_false() {
    let dir = make_test_dir("errlog3fail");
    // A path under a non-existent directory cannot be opened for appending.
    let badpath = dir.join("no_such_dir").join("app.log");
    let badstr = badpath.to_string_lossy().replace('\\', "\\\\");
    let src = format!(
        "<?php $ok = error_log(\"x\\n\", 3, \"{p}\"); echo $ok ? 'T' : 'F';",
        p = badstr
    );
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("F"),
        "error_log should return false when the file cannot be opened: {:?}",
        resp
    );
}

/// Verifies `trigger_error("x", E_WARNING)` returns true and does not corrupt the
/// HTTP response body: text echoed after the call still lands in the body. Also
/// proves the global `E_WARNING` constant resolves in compiled `--web` code.
#[test]
fn trigger_error_returns_true_and_preserves_body() {
    let dir = make_test_dir("trigerr");
    let src = "<?php $r = trigger_error(\"custom warning\", E_WARNING); echo $r ? 'RET_TRUE' : 'RET_FALSE'; echo '|body-after';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.contains("RET_TRUE|body-after"),
        "trigger_error must return true and leave the body intact: {:?}",
        resp
    );
}

/// Verifies a session-misuse warning reaches the worker's stderr: calling
/// `session_start()` twice emits the real PHP "already active" notice to stderr
/// while the HTTP body still renders. Uses stderr-to-file redirection so the
/// separate stderr channel can be asserted.
#[test]
fn session_start_twice_warns_on_stderr() {
    let dir = make_test_dir("sess_warn");
    let bin = compile_web(
        &dir,
        "<?php session_start(); session_start(); echo 'done';",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let stderr_file = dir.join("server.stderr");
    let mut child = spawn_server_stderr_to_file(&bin, &addr, "1", &stderr_file);
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.ends_with("done"),
        "body should still render after the notice: {:?}",
        resp
    );
    let logged = fs::read_to_string(&stderr_file).unwrap_or_default();
    assert!(
        logged.contains(
            "session_start(): Ignoring session_start() because a session is already active"
        ),
        "the already-active session notice should be written to the worker stderr, got: {:?}",
        logged
    );
}

/// End-to-end `session.upload_progress`: with `cleanup` OFF, a multipart POST
/// carrying a `PHP_SESSION_UPLOAD_PROGRESS` field followed by a file part causes
/// the prefork server's body-drain to write a progress entry into the session
/// file. A later request reads `$_SESSION['upload_progress_mykey']` and observes
/// the final `done => true` snapshot with the request content_length and one
/// files entry.
///
/// Because the server buffers a request's body before its own handler runs, the
/// upload request cannot observe its own intermediate progress, so this asserts
/// the END STATE via a separate follow-up request. Intermediate-progress
/// concurrency (a poll request racing the in-flight upload) is not exercised
/// here: the single-worker keep-alive harness serializes requests, so a
/// concurrent read cannot be driven deterministically.
#[test]
fn upload_progress_end_state_persists_with_cleanup_off() {
    let dir = make_test_dir("sess_upload_prog");
    let session_store = dir.join("sessions");
    fs::create_dir_all(&session_store).unwrap();
    let session_store_env = session_store.to_string_lossy().into_owned();
    // Request 1 starts a session; requests 2/3 branch on the URI path. Request 2
    // is a bare upload endpoint (no session_start) so ONLY the drain writes the
    // progress entry; request 3 starts the session and reads it back.
    let src = "<?php \
        if ($_SERVER['REQUEST_URI'] === '/start') { session_start(); echo 'started'; } \
        elseif ($_SERVER['REQUEST_URI'] === '/upload') { echo 'received'; } \
        else { \
            session_start(); \
            $p = $_SESSION['upload_progress_mykey'] ?? null; \
            if ($p === null) { echo 'MISSING'; } \
            else { \
                echo ($p['done'] ? 'done' : 'notdone'); \
                echo '|cl=' . $p['content_length']; \
                echo '|files=' . count($p['files']); \
                echo '|fn=' . $p['files'][0]['field_name']; \
            } \
        }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    // The deployment overrides must seed both the pre-handler upload tracker and
    // the later PHP session_start() calls. Cleanup stays off so the final snapshot
    // survives for the read request; php_serialize exercises path/format alignment.
    let mut child = spawn_server_with_env(
        &bin,
        &addr,
        "1",
        &[
            ("ELEPHC_SESSION_UPLOAD_PROGRESS_CLEANUP", "0"),
            ("ELEPHC_SESSION_NAME", "APPSESSID"),
            ("ELEPHC_SESSION_SAVE_PATH", &session_store_env),
            ("ELEPHC_SESSION_SERIALIZE_HANDLER", "php_serialize"),
        ],
    );

    // 1) Start the session, capture the cookie.
    let r1 = http_request(&addr, "GET", "/start", &[], "");
    let cookie = extract_session_id(&r1, "APPSESSID")
        .expect("first response should set the deployment-configured session cookie");
    let cookie_hdr = format!("APPSESSID={}", cookie);

    // 2) POST a multipart upload with the progress trigger field before the file.
    let boundary = "elephcUPLOADBOUND";
    let body = format!(
        "--{b}\r\n\
         Content-Disposition: form-data; name=\"PHP_SESSION_UPLOAD_PROGRESS\"\r\n\
         \r\n\
         mykey\r\n\
         --{b}\r\n\
         Content-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\
         \r\n\
         HELLO_UPLOAD_PAYLOAD\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let ctype = format!("multipart/form-data; boundary={}", boundary);
    let r2 = http_request(
        &addr,
        "POST",
        "/upload",
        &[("Cookie", &cookie_hdr), ("Content-Type", &ctype)],
        &body,
    );

    // 3) Read the persisted progress entry back.
    let r3 = http_request(&addr, "GET", "/read", &[("Cookie", &cookie_hdr)], "");

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        r2.contains("received"),
        "upload endpoint should respond: {:?}",
        r2
    );
    assert!(
        r3.contains("done"),
        "read request should see the progress entry with done=true: {:?}",
        r3
    );
    assert!(
        r3.contains(&format!("cl={}", body.len())),
        "content_length should equal the POST body length ({}): {:?}",
        body.len(),
        r3
    );
    assert!(r3.contains("files=1"), "one files entry expected: {:?}", r3);
    assert!(
        r3.contains("fn=f"),
        "file field_name should be 'f': {:?}",
        r3
    );
    assert!(
        session_store.join(format!("sess_{cookie}")).exists(),
        "upload tracker and session_start must share the configured save path"
    );
}

/// Verifies `session.use_trans_sid=1` (with `use_only_cookies=0`) URL-rewrites
/// the response for a cookie-less request: same-origin `<a href>` links gain the
/// `PHPSESSID` query parameter and `<form>` tags get a hidden SID input carrying
/// the session id. The same handler requested WITH the session cookie leaves the
/// body unrewritten (the id already round-trips via the cookie).
#[test]
fn trans_sid_rewrites_urls_and_forms() {
    let dir = make_test_dir("sess_transsid");
    let src = "<?php session_start(['use_trans_sid' => 1, 'use_only_cookies' => 0]); echo '<a href=\"/next\">n</a><form action=\"/post\"></form>';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");

    // First request: no session cookie → the body must be rewritten.
    let r1 = http_get(&addr, "/");
    let id = extract_phpsessid(&r1).expect("first response should set a PHPSESSID cookie");

    // Second request: send the cookie back → the body must be left unrewritten.
    let r2 = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={}", id))],
        "",
    );

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        r1.contains(&format!("/next?PHPSESSID={}", id)),
        "cookie-less response should carry the SID on the anchor: {:?}",
        r1
    );
    assert!(
        r1.contains(&format!(
            "<input type=\"hidden\" name=\"PHPSESSID\" value=\"{}\" />",
            id
        )),
        "cookie-less response should inject the hidden SID form field: {:?}",
        r1
    );
    assert!(
        r2.contains("<a href=\"/next\">") && !r2.contains("/next?PHPSESSID="),
        "cookie-carrying response should be left unrewritten: {:?}",
        r2
    );
    assert!(
        !r2.contains("type=\"hidden\""),
        "cookie-carrying response should not inject a hidden field: {:?}",
        r2
    );
}

/// Verifies the default configuration (omitting `use_only_cookies`, so it stays
/// `1`) never URL-rewrites, even with `use_trans_sid=1` and no session cookie:
/// URL propagation is opt-in and off by default.
#[test]
fn trans_sid_default_only_cookies_no_rewrite() {
    let dir = make_test_dir("sess_transsid_default");
    let src = "<?php session_start(['use_trans_sid' => 1]); echo '<a href=\"/next\">n</a>';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let resp = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        resp.contains("<a href=\"/next\">") && !resp.contains("/next?PHPSESSID="),
        "default use_only_cookies=1 must not URL-rewrite: {:?}",
        resp
    );
}

/// Verifies accepted cookies are not reissued and binary PHP strings survive
/// the session file and pointer/length bridge unchanged across requests.
#[test]
fn session_cookie_reuse_and_binary_string_round_trip() {
    let dir = make_test_dir("sess_binary_cookie");
    let src = "<?php session_start(); if (!isset($_SESSION['bin'])) { $_SESSION['bin'] = \"a\\0b\"; echo 'seed'; } else { echo strlen($_SESSION['bin']) . ':' . ord(substr($_SESSION['bin'], 1, 1)); }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let cookie = extract_phpsessid(&first).expect("new session should set a cookie");
    let second = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={cookie}"))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        second.ends_with("3:0"),
        "embedded NUL must round-trip: {second:?}"
    );
    assert!(
        !second.to_ascii_lowercase().contains("set-cookie:"),
        "an accepted cookie must not be reissued: {second:?}"
    );
}

/// Verifies a newly-created read-and-close session still sends its identifier
/// cookie even though its storage lock is closed before user code continues.
#[test]
fn session_read_and_close_new_session_sends_cookie() {
    let dir = make_test_dir("sess_read_close_cookie");
    let bin = compile_web(
        &dir,
        "<?php echo session_start(['read_and_close' => true]) ? session_status() : 9;",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        extract_phpsessid(&response).is_some(),
        "missing new cookie: {response:?}"
    );
    assert!(
        response.ends_with('1'),
        "read_and_close must end inactive: {response:?}"
    );
}

/// Verifies cookie-less GET and form-POST session transports resume a session
/// when cookies and the cookie-only policy are disabled.
#[test]
fn session_get_and_post_sid_transport_resumes_session() {
    let dir = make_test_dir("sess_get_sid");
    let src = "<?php session_start(['use_cookies' => false, 'use_only_cookies' => false, 'use_strict_mode' => false]); if (!isset($_SESSION['n'])) { $_SESSION['n'] = 0; } $_SESSION['n'] = $_SESSION['n'] + 1; echo session_id() . ':' . $_SESSION['n'];";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let body = first.rsplit("\r\n\r\n").next().unwrap_or("");
    let id = body.split(':').next().unwrap_or("");
    let second = http_get(&addr, &format!("/?PHPSESSID={id}"));
    let post_body = format!("PHPSESSID={id}");
    let third = http_request(
        &addr,
        "POST",
        "/",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        &post_body,
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        !id.is_empty(),
        "first response must expose an id: {first:?}"
    );
    assert!(
        second.ends_with(":2"),
        "GET SID must resume the counter: {second:?}"
    );
    assert!(
        third.ends_with(":3"),
        "POST SID must resume the counter: {third:?}"
    );
    assert!(
        extract_phpsessid(&first).is_none(),
        "use_cookies=0 must suppress cookies"
    );
}

/// Verifies inactive GC fails, matching php-src's lifecycle precondition.
#[test]
fn session_gc_requires_active_session() {
    let dir = make_test_dir("sess_gc_inactive");
    let bin = compile_web(
        &dir,
        "<?php echo session_gc() === false ? 'no' : 'yes';",
        "app",
    );
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        response.ends_with("no"),
        "inactive session_gc must fail: {response:?}"
    );
}

/// Verifies regeneration writes and closes the old record, opens a distinct
/// record, and persists subsequent shutdown data under the new identifier.
#[test]
fn session_regenerate_id_preserves_old_and_new_records() {
    let dir = make_test_dir("sess_regenerate");
    let src = "<?php session_start(['use_strict_mode' => false]); if (isset($_GET['regen'])) { $_SESSION['v'] = 'kept'; $old = session_id(); session_regenerate_id(false); echo $old . ':' . session_id(); } else { echo isset($_SESSION['v']) ? $_SESSION['v'] : 'empty'; }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let old = extract_phpsessid(&first).expect("initial cookie");
    let regenerated = http_request(
        &addr,
        "GET",
        "/?regen=1",
        &[("Cookie", &format!("PHPSESSID={old}"))],
        "",
    );
    let new = extract_phpsessid(&regenerated).expect("regeneration cookie");
    let old_response = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={old}"))],
        "",
    );
    let new_response = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={new}"))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert_ne!(old, new, "regeneration must create a distinct id");
    assert!(
        old_response.ends_with("kept"),
        "old record was not written: {old_response:?}"
    );
    assert!(
        new_response.ends_with("kept"),
        "new record was not persisted: {new_response:?}"
    );
}

/// Verifies a second `session_start()` in one request clears stale in-memory
/// keys before decoding the newly-selected storage record.
#[test]
fn session_restart_clears_stale_keys() {
    let dir = make_test_dir("sess_restart_clear");
    let src = "<?php session_id('firstsessionid'); session_start(['use_strict_mode' => false]); $_SESSION['stale'] = 'x'; session_write_close(); session_id('secondsessionid'); session_start(['use_strict_mode' => false]); echo isset($_SESSION['stale']) ? 'stale' : 'clean';";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        response.ends_with("clean"),
        "restart retained a stale key: {response:?}"
    );
}

/// Verifies current INI entries/defaults/access metadata and Partitioned cookie
/// emission with the required Secure attribute.
#[test]
fn session_ini_surface_and_partitioned_cookie() {
    let dir = make_test_dir("sess_ini_partitioned");
    let src = "<?php if (isset($_GET['bad'])) { if (!session_start(['save_path' => '/elephc/definitely/missing/session/path'])) { echo 'read-failed'; } } elseif (isset($_GET['insecure'])) { if (session_set_cookie_params(['partitioned' => true])) { echo 'set-ok:'; } if (!session_start()) { echo 'start-failed'; } } else { $access = -1; foreach (ini_get_all('session') as $key => $entry) { if ($key === 'session.auto_start') { foreach ($entry as $field => $value) { if ($field === 'access') { $access = $value; } } } } echo ini_get('session.save_handler') . ':' . ini_get('session.use_cookies') . ':' . ini_get('session.lazy_write') . ':' . $access . '|'; session_start(['name' => 'bad name', 'cookie_lifetime' => -1, 'cookie_secure' => true, 'cookie_partitioned' => true, 'cookie_samesite' => 'Bogus', 'serialize_handler' => 'bogus', 'gc_probability' => -1, 'gc_divisor' => 0]); echo ini_get('session.name') . ':' . ini_get('session.serialize_handler') . ':' . ini_get('session.gc_probability') . ':' . ini_get('session.gc_divisor'); }";
    let bin = compile_web(&dir, src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let failed_read = http_get(&addr, "/?bad=1");
    let insecure_partition = http_get(&addr, "/?insecure=1");
    let response = http_get(&addr, "/");
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        failed_read.ends_with("read-failed"),
        "files-handler read failure must make session_start fail: {failed_read:?}"
    );
    assert!(
        insecure_partition.ends_with("set-ok:start-failed"),
        "insecure Partitioned must fail at session start: {insecure_partition:?}"
    );
    assert!(
        response.contains("files:1:1:2|"),
        "INI surface mismatch: {response:?}"
    );
    assert!(
        response.ends_with("PHPSESSID:php:1:100"),
        "invalid start options changed session state: {response:?}"
    );
    let lower = response.to_ascii_lowercase();
    assert!(
        lower.contains("; secure") && lower.contains("; partitioned"),
        "partitioned cookie attributes missing: {response:?}"
    );
}

/// Verifies an unchanged custom-handler session uses `updateTimestamp()` under
/// lazy write, while the initial request uses `write()`.
#[test]
fn session_custom_handler_lazy_update_timestamp() {
    let dir = make_test_dir("sess_custom_lazy");
    let store = dir.join("store");
    fs::create_dir_all(&store).unwrap();
    let store = store.to_string_lossy().replace('\\', "/");
    let src = format!(
        "<?php
        class LazyHandler implements SessionHandlerInterface, SessionUpdateTimestampHandlerInterface {{
            public function open(string $path, string $name): bool {{ return true; }}
            public function close(): bool {{ return true; }}
            public function read(string $id): string|false {{
                $f = '{store}/' . $id;
                if (!file_exists($f)) {{ return ''; }}
                return file_get_contents($f);
            }}
            public function write(string $id, string $data): bool {{
                file_put_contents('{store}/' . $id, $data);
                file_put_contents('{store}/written', '1');
                return true;
            }}
            public function destroy(string $id): bool {{ return true; }}
            public function gc(int $max_lifetime): int|false {{ return 0; }}
            public function validateId(string $id): bool {{ return file_exists('{store}/' . $id); }}
            public function updateTimestamp(string $id, string $data): bool {{
                file_put_contents('{store}/updated', '1');
                return true;
            }}
        }}
        session_set_save_handler(new LazyHandler());
        session_start();
        if (!isset($_SESSION['stable'])) {{ $_SESSION['stable'] = 'value'; }}
        session_write_close();
        if (file_exists('{store}/updated')) {{ echo 'U'; }} else {{ echo 'W'; }}",
        store = store
    );
    let bin = compile_web(&dir, &src, "app");
    let port = free_port();
    let addr = format!("127.0.0.1:{}", port);
    let mut child = spawn_server(&bin, &addr, "1");
    let first = http_get(&addr, "/");
    let cookie = extract_phpsessid(&first).expect("custom handler should set a cookie");
    let second = http_request(
        &addr,
        "GET",
        "/",
        &[("Cookie", &format!("PHPSESSID={cookie}"))],
        "",
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(first.ends_with('W'), "first request must write: {first:?}");
    assert!(second.ends_with('U'), "unchanged request must update timestamp: {second:?}");
}
