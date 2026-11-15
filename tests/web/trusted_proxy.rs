//! Purpose:
//! End-to-end tests for the opt-in `--trusted-proxy` forwarded-header policy
//! across the worker, pool, and request isolation models.
//!
//! Called from:
//! - `tests/web_tests.rs` through Rust's integration test harness.
//!
//! Key details:
//! - The request source echoes the PHP-visible `$_SERVER` facts so each test can
//!   assert the exact REMOTE_ADDR / scheme / HTTPS / port derivation.
//! - The no-flag and untrusted-peer cases must remain byte-identical to the
//!   pre-feature behavior.

use super::*;

/// Echoes the `$_SERVER` keys involved in forwarded-header derivation.
const APP: &str = r#"<?php
$keys = [
    'REMOTE_ADDR', 'REMOTE_PORT', 'REQUEST_SCHEME', 'HTTPS', 'SERVER_PORT',
    'HTTP_HOST', 'HTTP_X_FORWARDED_FOR', 'HTTP_X_FORWARDED_PROTO', 'HTTP_FORWARDED',
];
foreach ($keys as $key) {
    echo $key, '=', array_key_exists($key, $_SERVER) ? $_SERVER[$key] : '<absent>', "\n";
}
"#;

/// Compiles the echo app for one isolation model and returns its fixture dir and binary.
fn compile_app(mode: Option<&str>) -> (PathBuf, PathBuf) {
    let fixture = make_test_dir("web_trusted_proxy");
    let flags: &[&str] = match mode {
        Some("pool") => &["--web-isolation=pool"],
        Some("request") => &["--web-isolation=request"],
        _ => &[],
    };
    let binary = compile_web_with_flags(&fixture, APP, "app", flags);
    (fixture, binary)
}

/// Spawns the app on an ephemeral TCP port, optionally with a trusted-proxy flag.
fn serve(binary: &Path, trusted: Option<&str>) -> (ServerGuard, String, u16) {
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let extra: Vec<&str> = trusted
        .map(|value| vec!["--trusted-proxy", value])
        .unwrap_or_default();
    let server = spawn_server_with_args(binary, &addr, "1", &extra);
    (server, addr, port)
}

/// Returns one echoed field from a response body.
fn field<'a>(body: &'a str, key: &str) -> &'a str {
    body.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key} in response:\n{body}"))
}

/// Asserts the fully-forwarded facts for a request sent through a trusted proxy.
fn assert_forwarded(body: &str, host: &str) {
    assert_eq!(field(body, "REMOTE_ADDR"), "203.0.113.9");
    assert_eq!(field(body, "REQUEST_SCHEME"), "https");
    assert_eq!(field(body, "HTTPS"), "on");
    assert_eq!(field(body, "SERVER_PORT"), "443");
    assert_eq!(field(body, "HTTP_HOST"), host);
    assert_eq!(field(body, "HTTP_X_FORWARDED_FOR"), "203.0.113.9");
    assert_eq!(field(body, "HTTP_X_FORWARDED_PROTO"), "https");
}

/// Asserts the unchanged baseline when forwarded headers are not trusted.
fn assert_untrusted(body: &str, host: &str, port: u16) {
    assert_eq!(field(body, "REMOTE_ADDR"), "127.0.0.1");
    assert_eq!(field(body, "REQUEST_SCHEME"), "http");
    assert_eq!(field(body, "HTTPS"), "<absent>");
    assert_eq!(field(body, "SERVER_PORT").parse::<u16>().unwrap(), port);
    assert_eq!(field(body, "HTTP_HOST"), host);
    assert_eq!(field(body, "HTTP_X_FORWARDED_FOR"), "203.0.113.9");
    assert_eq!(field(body, "HTTP_X_FORWARDED_PROTO"), "https");
}

/// Sends the standard proxy request with both X-Forwarded headers.
fn forwarded_request(addr: &str) -> String {
    http_request(
        addr,
        "GET",
        "/",
        &[
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https"),
        ],
        "",
    )
}

/// The default in-process worker derives the client facts from a trusted peer.
#[test]
fn trusted_proxy_worker_applies_forwarded_headers() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = forwarded_request(&addr);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_forwarded(raw_response_body(&response), &addr);
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// Without the flag, the same headers do not change `$_SERVER`.
#[test]
fn trusted_proxy_worker_ignores_headers_without_flag() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, port) = serve(&binary, None);
    let response = forwarded_request(&addr);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_untrusted(raw_response_body(&response), &addr, port);
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A peer outside the allowlist cannot supply forwarded headers.
#[test]
fn trusted_proxy_worker_ignores_untrusted_peer() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, port) = serve(&binary, Some("10.0.0.0/8"));
    let response = forwarded_request(&addr);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_untrusted(raw_response_body(&response), &addr, port);
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// The persistent handler pool sees the same trusted-proxy facts.
#[test]
fn trusted_proxy_pool_applies_forwarded_headers() {
    let (fixture, binary) = compile_app(Some("pool"));
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = forwarded_request(&addr);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_forwarded(raw_response_body(&response), &addr);
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A disposable handler child sees the same trusted-proxy facts.
#[test]
fn trusted_proxy_request_applies_forwarded_headers() {
    let (fixture, binary) = compile_app(Some("request"));
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = forwarded_request(&addr);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_forwarded(raw_response_body(&response), &addr);
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// Repeated X-Forwarded-For lines are merged; the appended proxy line wins.
#[test]
fn trusted_proxy_duplicate_forwarded_lines_use_the_rightmost_client() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("X-Forwarded-For", "6.6.6.6"),
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https"),
        ],
        "",
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(field(body, "REMOTE_ADDR"), "203.0.113.9");
    assert_eq!(field(body, "REQUEST_SCHEME"), "https");
    assert_eq!(field(body, "HTTPS"), "on");
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A same-line X-Forwarded-Proto list uses the rightmost token.
#[test]
fn trusted_proxy_same_line_proto_uses_the_rightmost_token() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https, http"),
        ],
        "",
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(field(body, "REQUEST_SCHEME"), "http");
    assert_eq!(field(body, "HTTPS"), "<absent>");
    assert_eq!(field(body, "SERVER_PORT"), "80");
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A present RFC 7239 header blocks a client-supplied X-Forwarded family.
#[test]
fn trusted_proxy_forwarded_presence_blocks_x_forwarded_fallback() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("Forwarded", "for=unknown;proto=https"),
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https"),
        ],
        "",
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(field(body, "REMOTE_ADDR"), "127.0.0.1");
    assert_eq!(field(body, "REQUEST_SCHEME"), "http");
    assert_eq!(field(body, "HTTPS"), "<absent>");
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A client-supplied X-Forwarded-Host port cannot override SERVER_PORT.
#[test]
fn trusted_proxy_x_forwarded_host_does_not_set_port() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let response = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("X-Forwarded-For", "203.0.113.9"),
            ("X-Forwarded-Proto", "https"),
            ("X-Forwarded-Host", "example.com:8443"),
        ],
        "",
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(field(body, "SERVER_PORT"), "443");
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// RFC 7239 `Forwarded` has precedence and supplies the host port.
#[test]
fn trusted_proxy_forwarded_rfc7239_wins() {
    let (fixture, binary) = compile_app(None);
    let (server, addr, _) = serve(&binary, Some("127.0.0.1/32"));
    let forwarded =
        "for=203.0.113.9;proto=https;host=example.com:8443";
    let response = http_request(
        &addr,
        "GET",
        "/",
        &[
            ("Forwarded", forwarded),
            ("X-Forwarded-For", "198.51.100.1"),
            ("X-Forwarded-Proto", "http"),
            ("X-Forwarded-Port", "9443"),
        ],
        "",
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body = raw_response_body(&response);
    assert_eq!(field(body, "REMOTE_ADDR"), "203.0.113.9");
    assert_eq!(field(body, "REQUEST_SCHEME"), "https");
    assert_eq!(field(body, "HTTPS"), "on");
    assert_eq!(field(body, "SERVER_PORT"), "8443");
    assert_eq!(field(body, "HTTP_FORWARDED"), forwarded);
    assert_eq!(field(body, "HTTP_X_FORWARDED_FOR"), "198.51.100.1");
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}

/// A malformed allowlist entry is a usage error before the server starts.
#[test]
fn trusted_proxy_rejects_malformed_flag() {
    let (fixture, binary) = compile_app(None);
    let output = output_with_timeout(
        {
            let mut command = Command::new(&binary);
            command
                .arg("--listen")
                .arg("127.0.0.1:0")
                .arg("--trusted-proxy")
                .arg("not-an-ip");
            command
        },
        Duration::from_secs(5),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--trusted-proxy"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(fixture);
}

/// Without the flag, the sorted `$_SERVER` key set matches the base-SHA list.
#[test]
fn trusted_proxy_golden_server_keys_without_flag() {
    let fixture = make_test_dir("web_trusted_proxy_golden");
    let source = r#"<?php
$keys = array_keys($_SERVER);
sort($keys);
echo implode(",", $keys);
"#;
    let binary = compile_web_with_flags(&fixture, source, "app", &[]);
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let server = spawn_server_with_args(&binary, &addr, "1", &[]);
    let response = http_get(&addr, "/");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert_eq!(
        raw_response_body(&response).trim(),
        "GATEWAY_INTERFACE,HTTP_CONNECTION,HTTP_HOST,QUERY_STRING,REMOTE_ADDR,\
         REMOTE_PORT,REQUEST_METHOD,REQUEST_SCHEME,REQUEST_TIME,REQUEST_URI,\
         SERVER_ADDR,SERVER_NAME,SERVER_PORT,SERVER_PROTOCOL,SERVER_SOFTWARE"
            .replace(' ', "")
    );
    drop(server);
    let _ = fs::remove_dir_all(fixture);
}
