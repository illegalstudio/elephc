//! Purpose:
//! The first real HTTP transfer through `curl_exec()`: a GET against the loopback fixture
//! in `http_fixture.rs`, both in `RETURNTRANSFER` and default (stdout) mode, plus the
//! connection-refused error shape over a genuine `http://` transfer.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - No fixture here reaches the public internet: every URL is `127.0.0.1` — either the
//!   fixture server's ephemeral port or a closed port (`:1`) for the failure case.
//! - `curl_get_returntransfer_localhost` is also `curl_getinfo()`'s first real test:
//!   `CURLINFO_HTTP_CODE` (2097154) is the only option this build understands (see
//!   `crate::curl_prelude`'s `curl_getinfo()`); the no-`$option` associative-array form and
//!   every other `CURLINFO_*` option are Task 8 Wave C's responsibility.
//! - Every prior curl transfer fixture (`easy_handle.rs`) used `file://` so the ownership
//!   and RETURNTRANSFER-capture plumbing could be proven without a socket. These three are
//!   the first to go over a REAL TCP connection, which is what actually exercises the write
//!   callback's streaming path end to end.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// A `RETURNTRANSFER` GET against the loopback fixture round-trips the response body AND
/// `curl_getinfo(..., CURLINFO_HTTP_CODE)`'s real HTTP status line, over an actual socket.
#[test]
fn curl_get_returntransfer_localhost() {
    if skip_without_curl_native("curl_get_returntransfer_localhost") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch);
        echo "\n", curl_getinfo($ch, CURLINFO_HTTP_CODE);
        "#
    ));
    assert_eq!(out, "hello-curl\n200");
}

/// Without `RETURNTRANSFER`, `curl_exec()` streams the body straight to stdout over a real
/// socket and returns `true` — the network-backed sibling of
/// `easy_handle.rs::transfer_without_returntransfer_writes_to_stdout`, which only proved
/// this over `file://`.
#[test]
fn curl_exec_writes_stdout_without_returntransfer() {
    if skip_without_curl_native("curl_exec_writes_stdout_without_returntransfer") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $ok = curl_exec($ch);
        echo $ok === true ? "T" : "F";
        "#
    ));
    assert_eq!(out, "hello-curlT");
}

/// A connection refused on a closed loopback port reports PHP's honest failure shape
/// (`false` + non-zero `curl_errno()` + non-empty `curl_error()`) for a REAL `http://`
/// transfer, complementing `easy_handle.rs::failed_transfer_reports_curl_error`.
#[test]
fn curl_exec_connection_refused_returns_false() {
    if skip_without_curl_native("curl_exec_connection_refused_returns_false") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X";
        echo curl_errno($ch) === 0 ? "0" : "E";
        echo strlen(curl_error($ch)) > 0 ? "M" : "N";
        "#,
    );
    assert_eq!(out, "FEM");
}

/// `curl_getinfo(..., CURLINFO_HTTP_CODE)`'s boxed `int` result is freshly allocated
/// (`RuntimeFnId::CurlEasyGetinfoLong`'s `Fresh` ownership category, `src/ir/runtime_fn.rs`)
/// and never aliases the `CurlHandle`'s own resource: a loop of real transfers plus
/// `curl_getinfo()` calls must leave the heap balanced, proving that categorization did
/// not introduce a leak (a `MayAliasArguments` mistake here would have kept the handle's
/// temporary alive past its real lifetime) NOR a double-free of the handle itself.
#[test]
fn curl_getinfo_result_does_not_leak_or_alias_the_handle() {
    if skip_without_curl_native("curl_getinfo_result_does_not_leak_or_alias_the_handle") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function fetch(): int {{
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_exec($ch);
            $code = curl_getinfo($ch, CURLINFO_HTTP_CODE);
            unset($ch);
            return $code;
        }}
        echo fetch(), fetch(), fetch(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "200200200\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "curl_getinfo() must not leak or double-free");
}
