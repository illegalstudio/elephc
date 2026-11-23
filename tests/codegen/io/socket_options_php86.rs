//! Purpose:
//! Runs PHP 8.6's socket context-option fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests socket_options_php86`.
//!
//! Key details:
//! - PHP 8.6 adds `so_reuseaddr` (disable-able default), `so_keepalive`,
//!   `tcp_keepidle`/`tcp_keepintvl`/`tcp_keepcnt`, `so_linger`, and the
//!   validated `so_rcvbuf`/`so_sndbuf` buffer sizes. Earlier profiles ignore
//!   the unknown keys.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php85)
}

/// Verifies every new 8.6 socket context option is accepted on a bound server.
#[test]
fn test_socket_options_accepted_php86() {
    let out = run86(
        r#"<?php
$ctx = stream_context_get_default();
stream_context_set_option($ctx, "socket", "so_reuseaddr", 0);
stream_context_set_option($ctx, "socket", "so_keepalive", 1);
stream_context_set_option($ctx, "socket", "so_linger", 1);
stream_context_set_option($ctx, "socket", "tcp_keepidle", 30);
stream_context_set_option($ctx, "socket", "tcp_keepintvl", 10);
stream_context_set_option($ctx, "socket", "tcp_keepcnt", 3);
stream_context_set_option($ctx, "socket", "so_rcvbuf", 65536);
stream_context_set_option($ctx, "socket", "so_sndbuf", 65536);
$srv = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies an out-of-range receive buffer size fails stream creation on 8.6 while
/// the earlier profile ignores the unknown option.
#[test]
fn test_socket_invalid_rcvbuf_fails_creation_php86() {
    let source = r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "so_rcvbuf", 0);
$srv = @stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#;
    assert_eq!(run86(source), "fail");
    assert_eq!(run85(source), "ok", "PHP 8.5 ignores the unknown 8.6 option");
}

/// Verifies an out-of-range send buffer size also fails creation on 8.6.
#[test]
fn test_socket_invalid_sndbuf_fails_creation_php86() {
    let source = r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "so_sndbuf", 2147483648);
$srv = @stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#;
    assert_eq!(run86(source), "fail");
}

/// Verifies the client path accepts linger/keepalive/buffer options end-to-end.
#[test]
fn test_socket_client_options_php86() {
    let out = run86(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "so_linger", 1);
stream_context_set_option(stream_context_get_default(), "socket", "so_keepalive", 1);
stream_context_set_option(stream_context_get_default(), "socket", "so_rcvbuf", 32768);
$srv = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($srv, false);
$client = stream_socket_client("tcp://" . $addr);
$conn = stream_socket_accept($srv);
echo is_resource($conn) ? "accepted" : "fail";
if ($conn) { fclose($conn); }
fclose($client);
fclose($srv);
"#,
    );
    assert_eq!(out, "accepted");
}
