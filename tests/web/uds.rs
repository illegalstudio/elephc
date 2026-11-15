//! Purpose:
//! End-to-end tests for `--listen unix:/PATH` across the worker, pool, request,
//! and persistent-application lifecycles.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - Socket paths are short `/tmp` paths because macOS `sun_path` is 104 bytes.
//! - The guard reaps the master and its workers on success and on panic.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Asserts a 200 response whose body contains `expected`.
fn assert_uds_ok(response: &str, expected: &str) {
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(raw_response_body(response).contains(expected), "{response}");
}

/// Compiles a one-line echo server and drives it through a Unix socket.
fn serve_uds_and_assert(stem: &str, flags: &[&str], expected: &str) {
    let fixture = make_test_dir("web_uds");
    let source = format!("<?php echo \"{expected}\";");
    let binary = compile_web_with_flags(&fixture, &source, stem, flags);
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "1", &[]);
    let response = http_get_unix(&path, "/");
    assert_uds_ok(&response, expected);
    drop(server);
    assert!(
        !path.exists(),
        "the master must unlink unix:{} on shutdown",
        path.display()
    );
    let _ = fs::remove_dir_all(&fixture);
}

/// The default in-process worker path serves requests over a Unix socket.
#[test]
fn web_uds_worker_serves_request() {
    serve_uds_and_assert("app", &[], "worker-uds");
}

/// The persistent handler-pool path serves requests over a Unix socket.
#[test]
fn web_uds_pool_serves_request() {
    serve_uds_and_assert("app", &["--web-isolation=pool"], "pool-uds");
}

/// The disposable handler path serves requests over a Unix socket.
#[test]
fn web_uds_request_serves_request() {
    serve_uds_and_assert("app", &["--web-isolation=request"], "request-uds");
}

/// The persistent application lifecycle runs its registered handler over UDS.
#[test]
fn web_uds_application_lifecycle_serves_request() {
    let fixture = make_test_dir("web_uds_application");
    let source = r#"<?php
use Elephc\Web\Request;
use Elephc\Web\Worker;
use Elephc\Web\WorkerApplication;

class Application implements WorkerApplication {
    public function handle(Request $request): void {
        echo "application-uds:" . $request->path;
    }

    public function shutdown(): void {}
}

Worker::register(new Application());
"#;
    let binary = compile_web_with_flags(
        &fixture,
        source,
        "app",
        &["--web-lifecycle=application", "--strict-php"],
    );
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "1", &[]);
    let response = http_get_unix(&path, "/hello");
    assert_uds_ok(&response, "application-uds:/hello");
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// The origin creates the socket group-readable/writable, never world-accessible.
#[test]
fn web_uds_socket_is_not_world_accessible() {
    let fixture = make_test_dir("web_uds_socket_mode");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"mode\";", "app", &[]);
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "1", &[]);
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o660, "socket mode must stay owner/group only");
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// A live socket is not stale: a second master must refuse to steal the path.
#[test]
fn web_uds_refuses_a_live_socket() {
    let fixture = make_test_dir("web_uds_live_socket");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"live\";", "app", &[]);
    let path = uds_path();
    let first = spawn_server_unix(&binary, &path, "1", &[]);
    let output = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command
                .arg("--listen")
                .arg(format!("unix:{}", path.display()));
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already in use"), "{stderr}");
    let response = http_get_unix(&path, "/");
    assert_uds_ok(&response, "live");
    drop(first);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// Two workers accept from the same inherited Unix listener without duplicate
/// or dropped requests.
#[test]
fn web_uds_multiple_workers_serve_requests() {
    let fixture = make_test_dir("web_uds_workers");
    let binary = compile_web_with_flags(
        &fixture,
        "<?php echo \"workers-uds\";",
        "app",
        &[],
    );
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "2", &[]);
    for request in 0..20 {
        let response = http_get_unix(&path, "/");
        assert_uds_ok(&response, "workers-uds");
        assert!(
            response.starts_with("HTTP/1.1 200"),
            "request {request} failed: {response}"
        );
    }
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// A UDS peer has no IP: the PHP-facing server values are empty or zero.
#[test]
fn web_uds_server_values_are_empty() {
    let fixture = make_test_dir("web_uds_server_values");
    let source = r#"<?php
echo 'addr=[' . $_SERVER['REMOTE_ADDR'] . '] server_addr=[' . $_SERVER['SERVER_ADDR']
    . '] name=[' . $_SERVER['SERVER_NAME'] . '] port=[' . $_SERVER['REMOTE_PORT']
    . '] server=[' . $_SERVER['SERVER_PORT'] . ']';"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &[]);
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "1", &[]);
    let response = http_get_unix(&path, "/");
    assert_uds_ok(
        &response,
        "addr=[] server_addr=[] name=[] port=[0] server=[0]",
    );
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// A stale socket file left by a killed process is replaced at startup.
#[test]
fn web_uds_stale_socket_is_replaced() {
    let fixture = make_test_dir("web_uds_stale");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"stale-uds\";", "app", &[]);
    let path = uds_path();
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);
    let server = spawn_server_unix(&binary, &path, "1", &[]);
    let response = http_get_unix(&path, "/");
    assert_uds_ok(&response, "stale-uds");
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}

/// A non-socket file at the listen path is refused, not deleted.
#[test]
fn web_uds_refuses_a_non_socket_path() {
    let fixture = make_test_dir("web_uds_non_socket");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"x\";", "app", &[]);
    let path = uds_path();
    fs::write(&path, b"do not delete me").unwrap();
    let output = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command
                .arg("--listen")
                .arg(format!("unix:{}", path.display()));
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a Unix socket"), "{stderr}");
    assert_eq!(fs::read(&path).unwrap(), b"do not delete me");
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&fixture);
}

/// A missing parent directory fails in the master, not in the worker crash loop.
#[test]
fn web_uds_missing_directory_fails_in_master() {
    let fixture = make_test_dir("web_uds_missing_dir");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"x\";", "app", &[]);
    let path = "/nonexistent/elephc-test/dir/app.sock";
    let output = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command
                .arg("--listen")
                .arg(format!("unix:{path}"));
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(path), "{stderr}");
    assert!(!stderr.contains("workers died on startup"), "{stderr}");
    let _ = fs::remove_dir_all(&fixture);
}

/// Empty and oversized Unix paths are usage errors with a clear message.
#[test]
fn web_uds_rejects_empty_and_oversized_paths() {
    let fixture = make_test_dir("web_uds_usage");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"x\";", "app", &[]);

    let empty = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command.arg("--listen").arg("unix:");
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(empty.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&empty.stderr).contains("non-empty"));

    let oversized = format!("unix:/{}", "x".repeat(101));
    let output = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command.arg("--listen").arg(&oversized);
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("maximum"));

    let _ = fs::remove_dir_all(&fixture);
}

/// SIGTERM is the shutdown path that must remove the socket file.
#[test]
fn web_uds_sigterm_removes_socket() {
    let fixture = make_test_dir("web_uds_sigterm");
    let binary = compile_web_with_flags(&fixture, "<?php echo \"sigterm-uds\";", "app", &[]);
    let path = uds_path();
    let mut server = spawn_server_unix(&binary, &path, "1", &[]);
    assert!(path.exists(), "the socket must exist while the server runs");
    server.kill().unwrap();
    server.wait().unwrap();
    assert!(!path.exists(), "SIGTERM must unlink the socket");
    let _ = fs::remove_dir_all(&fixture);
}

/// Echoes the trusted-proxy facts for the combined UDS tests.
const PROXY_APP: &str = r#"<?php
$keys = [
    'REMOTE_ADDR', 'REQUEST_SCHEME', 'HTTPS', 'SERVER_PORT',
    'HTTP_X_FORWARDED_FOR', 'HTTP_X_FORWARDED_PROTO', 'HTTP_FORWARDED',
];
foreach ($keys as $key) {
    echo $key, '=', array_key_exists($key, $_SERVER) ? $_SERVER[$key] : '<absent>', "\n";
}
"#;

/// Returns one echoed field from a UDS proxy response.
fn proxy_field<'a>(body: &'a str, key: &str) -> &'a str {
    body.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in response:\n{body}"))
}

/// Asserts the forwarded facts derived from a trusted Unix peer.
fn assert_uds_forwarded(body: &str) {
    assert_eq!(proxy_field(body, "REMOTE_ADDR"), "203.0.113.9");
    assert_eq!(proxy_field(body, "REQUEST_SCHEME"), "https");
    assert_eq!(proxy_field(body, "HTTPS"), "on");
    assert_eq!(proxy_field(body, "SERVER_PORT"), "443");
    assert_eq!(proxy_field(body, "HTTP_X_FORWARDED_FOR"), "203.0.113.9");
    assert_eq!(proxy_field(body, "HTTP_X_FORWARDED_PROTO"), "https");
}

/// Asserts the unchanged UDS baseline when the peer is not trusted.
fn assert_uds_untrusted(body: &str) {
    assert_eq!(proxy_field(body, "REMOTE_ADDR"), "");
    assert_eq!(proxy_field(body, "REQUEST_SCHEME"), "http");
    assert_eq!(proxy_field(body, "HTTPS"), "<absent>");
    assert_eq!(proxy_field(body, "SERVER_PORT"), "0");
    assert_eq!(proxy_field(body, "HTTP_X_FORWARDED_FOR"), "203.0.113.9");
    assert_eq!(proxy_field(body, "HTTP_X_FORWARDED_PROTO"), "https");
}

/// Sends the standard proxy headers over a Unix socket.
fn uds_forwarded_request(path: &Path) -> String {
    http_request_unix(
        path,
        "GET",
        "/",
        &[
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https"),
        ],
        "",
    )
}

/// Runs the trusted and untrusted UDS cases for one isolation model.
fn assert_uds_trusted_mode(mode: Option<&str>) {
    let fixture = make_test_dir("web_uds_trusted");
    let flags: &[&str] = match mode {
        Some("pool") => &["--web-isolation=pool"],
        Some("request") => &["--web-isolation=request"],
        _ => &[],
    };
    let binary = compile_web_with_flags(&fixture, PROXY_APP, "app", flags);

    let trusted_path = uds_path();
    let trusted = spawn_server_unix(&binary, &trusted_path, "1", &["--trusted-proxy", "unix"]);
    let response = uds_forwarded_request(&trusted_path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_uds_forwarded(raw_response_body(&response));
    drop(trusted);
    assert!(!trusted_path.exists());

    let plain_path = uds_path();
    let plain = spawn_server_unix(&binary, &plain_path, "1", &[]);
    let response = uds_forwarded_request(&plain_path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_uds_untrusted(raw_response_body(&response));
    drop(plain);
    assert!(!plain_path.exists());

    let _ = fs::remove_dir_all(&fixture);
}

/// The default worker derives trusted facts from a Unix peer.
#[test]
fn web_uds_trusted_proxy_worker() {
    assert_uds_trusted_mode(None);
}

/// The pool model derives trusted facts from a Unix peer.
#[test]
fn web_uds_trusted_proxy_pool() {
    assert_uds_trusted_mode(Some("pool"));
}

/// The request model derives trusted facts from a Unix peer.
#[test]
fn web_uds_trusted_proxy_request() {
    assert_uds_trusted_mode(Some("request"));
}

/// The persistent application lifecycle derives trusted facts from a Unix peer.
#[test]
fn web_uds_trusted_proxy_application() {
    let fixture = make_test_dir("web_uds_trusted_application");
    let source = r#"<?php
use Elephc\Web\Request;
use Elephc\Web\Worker;
use Elephc\Web\WorkerApplication;

class Application implements WorkerApplication {
    public function handle(Request $request): void {
        $keys = [
            'REMOTE_ADDR', 'REQUEST_SCHEME', 'HTTPS', 'SERVER_PORT',
            'HTTP_X_FORWARDED_FOR', 'HTTP_X_FORWARDED_PROTO',
        ];
        foreach ($keys as $key) {
            echo $key, '=', array_key_exists($key, $_SERVER) ? $_SERVER[$key] : '<absent>', "\n";
        }
    }

    public function shutdown(): void {}
}

Worker::register(new Application());
"#;
    let binary = compile_web_with_flags(
        &fixture,
        source,
        "app",
        &["--web-lifecycle=application", "--strict-php"],
    );

    let trusted_path = uds_path();
    let trusted = spawn_server_unix(&binary, &trusted_path, "1", &["--trusted-proxy", "unix"]);
    let response = uds_forwarded_request(&trusted_path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_uds_forwarded(raw_response_body(&response));
    drop(trusted);
    assert!(!trusted_path.exists());

    let plain_path = uds_path();
    let plain = spawn_server_unix(&binary, &plain_path, "1", &[]);
    let response = uds_forwarded_request(&plain_path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_uds_untrusted(raw_response_body(&response));
    drop(plain);
    assert!(!plain_path.exists());

    let _ = fs::remove_dir_all(&fixture);
}

/// RFC 7239 `Forwarded` works over a trusted Unix peer and supplies the host port.
#[test]
fn web_uds_trusted_proxy_forwarded_rfc7239() {
    let fixture = make_test_dir("web_uds_trusted_forwarded");
    let binary = compile_web_with_flags(&fixture, PROXY_APP, "app", &[]);
    let path = uds_path();
    let server = spawn_server_unix(&binary, &path, "1", &["--trusted-proxy", "unix"]);
    let forwarded = "for=203.0.113.9;proto=https;host=example.com:8443";
    let response = http_request_unix(&path, "GET", "/", &[("Forwarded", forwarded)], "");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(proxy_field(body, "REMOTE_ADDR"), "203.0.113.9");
    assert_eq!(proxy_field(body, "REQUEST_SCHEME"), "https");
    assert_eq!(proxy_field(body, "HTTPS"), "on");
    assert_eq!(proxy_field(body, "SERVER_PORT"), "8443");
    assert_eq!(proxy_field(body, "HTTP_FORWARDED"), forwarded);
    drop(server);
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&fixture);
}
