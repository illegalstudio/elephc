//! Purpose:
//! Regression fixtures ported from php-src's `ext/curl/tests` suite for behaviors
//! uncovered while auditing elephc's `ext/curl` implementation against upstream.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - Each fixture names the upstream `.phpt` it mirrors so the audit trail is explicit.
//! - `compile_and_run_capture` is used when the PHP-observable behavior is a
//!   runtime diagnostic (elephc renders diagnostics on stderr, as the rest of the
//!   curl suite already asserts).
//! - Every fixture skips when the managed native `curl` package is absent, matching
//!   the rest of `tests/codegen/curl`.

use crate::support::*;

use super::http_fixture::LocalHttpServer;

/// php-src `curl_getinfo_CURLINFO_QUEUE_TIME_T.phpt` / `..._USED_PROXY.phpt`: the PHP 8.5
/// info keys (`queue_time_us`, `used_proxy`, `conn_id`, `httpauth_used`) are present in
/// the array form before a transfer, at 0.
#[test]
fn curl_getinfo_array_exposes_php85_keys() {
    if skip_without_curl_native("curl_getinfo_array_exposes_php85_keys") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        $info = curl_getinfo($ch);
        foreach (["queue_time_us", "used_proxy", "conn_id", "httpauth_used"] as $key) {
            echo isset($info[$key]) ? $key . "\n" : ("no-" . $key . "\n");
        }
        "#,
    );
    assert_eq!(out, "queue_time_us\nused_proxy\nconn_id\nhttpauth_used\n");
}

/// php-src `curl_multi_errno_strerror_001.phpt` / `curl_share_errno_strerror_001.phpt`: an
/// invalid option records `CURLM_UNKNOWN_OPTION` / `CURLSHE_BAD_OPTION` before the
/// `ValueError`.
#[test]
fn curl_multi_and_share_errno_record_invalid_options() {
    if skip_without_curl_native("curl_multi_and_share_errno_record_invalid_options") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $mh = curl_multi_init();
        try { curl_multi_setopt($mh, -1, -1); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
        echo curl_multi_errno($mh), "\n";
        $sh = curl_share_init();
        try { curl_share_setopt($sh, -1, -1); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
        echo curl_share_errno($sh), "\n";
        "#,
    );
    assert_eq!(
        out,
        "curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option\n6\n\
         curl_share_setopt(): Argument #2 ($option) is not a valid cURL share option\n1\n"
    );
}

/// php-src `curl_setopt_callables.phpt` / `curl_ssh_hostkey_invalid_callable.phpt`: callback
/// options elephc cannot invoke are still validated, so a bad value raises PHP's callback
/// `TypeError` rather than the unsupported-option warning.
#[test]
fn curl_setopt_validates_unimplemented_callback_options() {
    if skip_without_curl_native("curl_setopt_validates_unimplemented_callback_options") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        foreach ([CURLOPT_FNMATCH_FUNCTION, CURLOPT_SSH_HOSTKEYFUNCTION] as $option) {
            try { curl_setopt($ch, $option, "undefined"); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
        }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): Argument #3 ($value) must be a valid callback for option CURLOPT_FNMATCH_FUNCTION, function \"undefined\" not found or invalid function name\n\
         curl_setopt(): Argument #3 ($value) must be a valid callback for option CURLOPT_SSH_HOSTKEYFUNCTION, function \"undefined\" not found or invalid function name\n"
    );
}

/// php-src `bug79033.phpt` / `curl_copy_handle_basic_009.phpt`: `CURLINFO_HEADER_OUT` as
/// a `curl_setopt()` option captures the outgoing request line/headers, readable with
/// `curl_getinfo($ch, CURLINFO_HEADER_OUT)` and the array form's `request_header` key.
#[test]
fn curl_info_header_out_captures_the_request() {
    if skip_without_curl_native("curl_info_header_out_captures_the_request") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLINFO_HEADER_OUT, true);
        curl_exec($ch);
        $h = curl_getinfo($ch, CURLINFO_HEADER_OUT);
        echo str_starts_with($h, "GET /hello HTTP/1.1") ? "header\n" : "no-header\n";
        $info = curl_getinfo($ch);
        echo isset($info["request_header"]) ? "array\n" : "no-array\n";
        "#
    ));
    assert_eq!(out, "header\narray\n");
}

/// php-src `bug66109.phpt`: a nullable string option reset with `null` forwards libcurl's
/// own `NULL` (a reset), not an empty string — here `CURLOPT_CUSTOMREQUEST` reverts to GET.
#[test]
fn curl_setopt_null_resets_nullable_string_options() {
    if skip_without_curl_native("curl_setopt_null_resets_nullable_string_options") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_CUSTOMREQUEST, "DELETE");
        $b1 = curl_exec($ch);
        echo str_contains($b1, "method=DELETE") ? "delete\n" : "no-delete\n";
        curl_setopt($ch, CURLOPT_CUSTOMREQUEST, null);
        $b2 = curl_exec($ch);
        echo str_contains($b2, "method=GET") ? "get\n" : "no-get\n";
        "#
    ));
    assert_eq!(out, "delete\nget\n");
}


/// php-src `bug80121.phpt`: `new CurlHandle` / `CurlMultiHandle` / `CurlShareHandle`
/// must throw a catchable `Error` at runtime instead of being a compile-time
/// private-constructor failure.
#[test]
fn curl_handles_throw_on_direct_construction() {
    if skip_without_curl_native("curl_handles_throw_on_direct_construction") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        try { new CurlHandle; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
        try { new CurlMultiHandle; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
        try { new CurlShareHandle; } catch (\Error $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "Cannot directly construct CurlHandle, use curl_init() instead\n\
         Cannot directly construct CurlMultiHandle, use curl_multi_init() instead\n\
         Cannot directly construct CurlShareHandle, use curl_share_init() instead\n"
    );
}

/// php-src `bug63795.phpt`: `CURLOPT_SSL_VERIFYHOST === 1` warns and substitutes 2.
/// Values 0 and 2 are accepted silently.
#[test]
fn curl_setopt_ssl_verifyhost_one_is_rewritten_to_two() {
    if skip_without_curl_native("curl_setopt_ssl_verifyhost_one_is_rewritten_to_two") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        $ch = curl_init();
        var_dump(curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, false));
        var_dump(curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, true));
        var_dump(curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, 0));
        var_dump(curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, 1));
        var_dump(curl_setopt($ch, CURLOPT_SSL_VERIFYHOST, 2));
        "#,
    );
    assert_eq!(
        output.stdout,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(true)\n"
    );
    let notices = output
        .stderr
        .matches("CURLOPT_SSL_VERIFYHOST no longer accepts the value 1")
        .count();
    assert_eq!(notices, 2, "stderr was: {}", output.stderr);
}

/// php-src `bug68089.phpt`: an embedded NUL in a string option raises PHP's
/// `ValueError` before libcurl is reached.
#[test]
fn curl_setopt_rejects_embedded_null_bytes() {
    if skip_without_curl_native("curl_setopt_rejects_embedded_null_bytes") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            curl_setopt($ch, CURLOPT_URL, "file:///etc/passwd\0http://example.com");
        } catch (\ValueError $e) {
            echo $e->getMessage(), "\n";
        }
        echo "Done\n";
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): cURL option must not contain any null bytes\nDone\n"
    );
}

/// php-src `curl_persistent_share_003/004/006.phpt`: `curl_share_init_persistent()`
/// validation order, PHP's exact messages, and the sorted/de-duplicated `$options`
/// property.
#[test]
fn curl_share_init_persistent_validates_and_exposes_options() {
    if skip_without_curl_native("curl_share_init_persistent_validates_and_exposes_options") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        try { curl_share_init_persistent([]); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
        try { curl_share_init_persistent([CURL_LOCK_DATA_COOKIE]); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
        try { curl_share_init_persistent([CURL_LOCK_DATA_DNS, CURL_LOCK_DATA_CONNECT, 30]); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
        $sh = curl_share_init_persistent([CURL_LOCK_DATA_CONNECT, CURL_LOCK_DATA_DNS, CURL_LOCK_DATA_DNS]);
        var_dump($sh->options);
        "#,
    );
    assert_eq!(
        out,
        "curl_share_init_persistent(): Argument #1 ($share_options) must not be empty\n\
         curl_share_init_persistent(): Argument #1 ($share_options) must not contain CURL_LOCK_DATA_COOKIE because sharing cookies across PHP requests is unsafe\n\
         curl_share_init_persistent(): Argument #1 ($share_options) must contain only CURL_LOCK_DATA_* constants\n\
         array(2) {\n  [0]=>\n  int(3)\n  [1]=>\n  int(5)\n}\n"
    );
}

/// php-src `curl_persistent_share_005.phpt`: `curl_share_setopt()` accepts only a
/// `CurlShareHandle`, rejecting a persistent share (and any other value) with PHP's
/// runtime `TypeError`.
#[test]
fn curl_share_setopt_rejects_non_share_values_at_runtime() {
    if skip_without_curl_native("curl_share_setopt_rejects_non_share_values_at_runtime") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        try { curl_share_setopt("nope", CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
        try { curl_share_setopt(curl_share_init_persistent([CURL_LOCK_DATA_DNS]), CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "curl_share_setopt(): Argument #1 ($share_handle) must be of type CurlShareHandle, string given\n\
         curl_share_setopt(): Argument #1 ($share_handle) must be of type CurlShareHandle, CurlSharePersistentHandle given\n"
    );
}

/// php's `curl_setopt(CurlHandle $handle, ...)` rejects a non-handle at RUN time — the
/// fixture callbacks receive `$ch` as an untyped parameter, so the parameter is untyped
/// here too and the guard answers PHP's own message (measured against PHP 8.5:
/// `curl_setopt(): Argument #1 ($handle) must be of type CurlHandle, int given`).
#[test]
fn curl_setopt_rejects_non_handle_values_at_runtime() {
    if skip_without_curl_native("curl_setopt_rejects_non_handle_values_at_runtime") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        try { curl_setopt(1, CURLOPT_URL, "http://127.0.0.1/"); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
        try { curl_setopt("nope", CURLOPT_URL, "http://127.0.0.1/"); } catch (\TypeError $e) { echo $e->getMessage(), "\n"; }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): Argument #1 ($handle) must be of type CurlHandle, int given\n\
         curl_setopt(): Argument #1 ($handle) must be of type CurlHandle, string given\n"
    );
}

#[test]
fn curl_close_and_share_close_are_deprecated_but_not_multi_close() {
    if skip_without_curl_native("curl_close_and_share_close_are_deprecated_but_not_multi_close") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        curl_close(curl_init());
        curl_share_close(curl_share_init());
        curl_multi_close(curl_multi_init());
        "#,
    );
    assert!(
        output
            .stderr
            .contains("Function curl_close() is deprecated since 8.5"),
        "stderr was: {}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("Function curl_share_close() is deprecated since 8.5"),
        "stderr was: {}",
        output.stderr
    );
    assert!(
        !output.stderr.contains("curl_multi_close() is deprecated"),
        "curl_multi_close must not be deprecated; stderr was: {}",
        output.stderr
    );
}

/// php-src `curl_file_serialize.phpt`, caught variant: the synthetic builtin frame belongs
/// to the throwable that published it.
///
/// `serialize(new CURLFile(...))` raises an Exception whose trace php names as
/// `#0 file(line): serialize(Object(CURLFile))`. When the program CATCHES that exception, a
/// later unrelated uncaught throw must not inherit the frame — the flag that publishes it is
/// cleared with the bound exception.
#[test]
fn a_caught_builtin_throw_does_not_leak_its_frame_into_a_later_report() {
    if skip_without_curl_native("a_caught_builtin_throw_does_not_leak_its_frame_into_a_later_report") {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
        try { serialize(new CURLFile(__FILE__)); } catch (Throwable $e) {}
        throw new RuntimeException("later");
        "#,
    );
    assert!(!out.success, "the second throw must stay uncaught");
    assert!(
        out.stdout.contains("Fatal error: Uncaught RuntimeException: later"),
        "unexpected report: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("#0 {main}"),
        "the report must still carry the trace block: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("serialize(Object(CURLFile))"),
        "a caught exception's synthetic builtin frame leaked into a later report: {}",
        out.stdout
    );
}

/// php-src `curl_file_serialize.phpt`, rethrow variants: the synthetic builtin frame travels
/// WITH its throwable.
///
/// The frame is printed only while the throwable that published it is the one escaping, so a
/// caught exception cannot lend it to a later failure (see the test above) while `finally`
/// and an explicit catch-and-rethrow keep it.
#[test]
fn a_rethrown_builtin_throw_keeps_its_synthetic_frame() {
    if skip_without_curl_native("a_rethrown_builtin_throw_keeps_its_synthetic_frame") {
        return;
    }
    for source in [
        r#"<?php
        try { serialize(new CURLFile(__FILE__)); } finally { echo "finally\n"; }
        "#,
        r#"<?php
        try { serialize(new CURLFile(__FILE__)); } catch (Throwable $e) { throw $e; }
        "#,
    ] {
        let out = compile_and_run_capture(source);
        assert!(!out.success, "the throw must stay uncaught");
        assert!(
            out.stdout.contains("serialize(Object(CURLFile))"),
            "a rethrown builtin error must keep its synthetic frame: {}",
            out.stdout
        );
    }
}
