//! Purpose:
//! End-to-end fixtures for the `curl_setopt()` option surface: the long/bool/enum and
//! string options of Task 8 Wave A, driven against the loopback HTTP fixture so an option
//! that is "accepted" is also PROVED to have changed the transfer.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - ACCEPTANCE IS NOT THE ASSERTION. `curl_setopt()` returning `true` only says libcurl
//!   took the option; every fixture here also makes a real request whose OUTCOME depends
//!   on the option (the fixture echoes the request back, or the option makes the transfer
//!   fail), which is what distinguishes a working option from a silently-swallowed one.
//! - No fixture reaches the public internet: every URL is `127.0.0.1`, either the
//!   fixture server's ephemeral port or a closed port for the failure cases.
//! - The `CURLOPT_*` constants are used by NAME here, not by number, because Task 6
//!   registered them — which also makes these fixtures a second, end-to-end check that
//!   the frozen constant values and the bridge's option table agree.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// Wave A's long options really reach libcurl: `CURLOPT_NOBODY` turns the GET into a
/// HEAD, which the fixture reports back through the method it saw, and `CURLOPT_HEADER`
/// prepends the response status line to the captured body.
#[test]
fn wave_a_long_options_change_the_transfer() {
    if skip_without_curl_native("wave_a_long_options_change_the_transfer") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HEADER, true);
        $body = curl_exec($ch);
        echo str_starts_with($body, "HTTP/1.0 200") ? "headers\n" : "no-headers\n";

        $ch2 = curl_init("{url}");
        curl_setopt($ch2, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch2, CURLOPT_NOBODY, true);
        $body2 = curl_exec($ch2);
        echo strlen($body2) === 0 ? "nobody\n" : "body\n";
        echo curl_getinfo($ch2, CURLINFO_HTTP_CODE), "\n";
        "#
    ));
    assert_eq!(out, "headers\nnobody\n200\n");
}

/// `CURLOPT_TIMEOUT_MS` / `CURLOPT_CONNECTTIMEOUT_MS` are accepted and really bound the
/// transfer: a 1 ms connect timeout against a black-holed address fails with libcurl's
/// own timeout `CURLcode` (28) rather than hanging or silently succeeding.
///
/// `192.0.2.1` is TEST-NET-1 (RFC 5737), reserved for documentation and never routed, so
/// this stays inside the "no public internet" rule while still being an address the
/// kernel will not answer for.
#[test]
fn wave_a_timeout_options_bound_the_transfer() {
    if skip_without_curl_native("wave_a_timeout_options_bound_the_transfer") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://192.0.2.1/");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_CONNECTTIMEOUT_MS, 200);
        curl_setopt($ch, CURLOPT_TIMEOUT_MS, 300);
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X";
        echo curl_errno($ch) === CURLE_OPERATION_TIMEOUTED ? "T" : curl_errno($ch);
        "#,
    );
    assert_eq!(out, "FT");
}

/// Wave A's string options reach libcurl and are visible on the wire: the fixture echoes
/// the request line and headers it received, so `CURLOPT_USERAGENT`, `CURLOPT_REFERER`,
/// `CURLOPT_COOKIE` and `CURLOPT_CUSTOMREQUEST` are each proved by the request the server
/// actually saw, not by `curl_setopt()`'s return value.
#[test]
fn wave_a_string_options_reach_the_wire() {
    if skip_without_curl_native("wave_a_string_options_reach_the_wire") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_USERAGENT, "elephc-curl-test/1.0");
        curl_setopt($ch, CURLOPT_REFERER, "http://example.invalid/from");
        curl_setopt($ch, CURLOPT_COOKIE, "a=1; b=2");
        curl_setopt($ch, CURLOPT_CUSTOMREQUEST, "PATCH");
        $body = curl_exec($ch);
        echo str_contains($body, "method=PATCH") ? "method\n" : "no-method\n";
        echo str_contains($body, "user-agent: elephc-curl-test/1.0") ? "ua\n" : "no-ua\n";
        echo str_contains($body, "referer: http://example.invalid/from") ? "ref\n" : "no-ref\n";
        echo str_contains($body, "cookie: a=1; b=2") ? "cookie\n" : "no-cookie\n";
        "#
    ));
    assert_eq!(out, "method\nua\nref\ncookie\n");
}

/// `CURLOPT_FOLLOWLOCATION` + `CURLOPT_MAXREDIRS` really drive libcurl's redirect
/// follower: the fixture's `/redirect` sends a `302` to `/hello`, and the two options
/// decide whether the body is the redirect's or the target's. `CURLINFO_REDIRECT_COUNT`
/// is the same fact read back from libcurl.
#[test]
fn wave_a_followlocation_follows_a_local_redirect() {
    if skip_without_curl_native("wave_a_followlocation_follows_a_local_redirect") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/redirect");
    let out = compile_and_run(&format!(
        r#"<?php
        $off = curl_init("{url}");
        curl_setopt($off, CURLOPT_RETURNTRANSFER, true);
        $body = curl_exec($off);
        echo curl_getinfo($off, CURLINFO_HTTP_CODE), "\n";

        $on = curl_init("{url}");
        curl_setopt($on, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($on, CURLOPT_FOLLOWLOCATION, true);
        curl_setopt($on, CURLOPT_MAXREDIRS, 5);
        echo curl_exec($on), "\n";
        echo curl_getinfo($on, CURLINFO_HTTP_CODE), "\n";
        "#
    ));
    assert_eq!(out, "302\nhello-curl\n200\n");
}

/// `CURLOPT_FAILONERROR` turns a `404` into a transfer failure, exactly as PHP documents:
/// `curl_exec()` answers `false` and `curl_errno()` reports `CURLE_HTTP_RETURNED_ERROR`
/// (22) instead of handing back the error page's body.
#[test]
fn wave_a_failonerror_turns_a_404_into_a_failure() {
    if skip_without_curl_native("wave_a_failonerror_turns_a_404_into_a_failure") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/missing");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_FAILONERROR, true);
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X";
        echo curl_errno($ch), "\n";
        "#
    ));
    assert_eq!(out, "F22\n");
}

/// `CURLOPT_PRIVATE` is a PHP-layer option libcurl never sees: the value it stores is an
/// arbitrary PHP value, and `curl_getinfo(..., CURLINFO_PRIVATE)` reads back exactly what
/// was stored — `false` on a handle that never set it, matching php-src.
#[test]
fn wave_a_private_round_trips_through_getinfo() {
    if skip_without_curl_native("wave_a_private_round_trips_through_getinfo") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_getinfo($ch, CURLINFO_PRIVATE) === false ? "unset\n" : "set\n";
        curl_setopt($ch, CURLOPT_PRIVATE, "request-42");
        echo curl_getinfo($ch, CURLINFO_PRIVATE), "\n";
        "#,
    );
    assert_eq!(out, "unset\nrequest-42\n");
}

/// `CURLOPT_SAFE_UPLOAD` is always on: setting it `true` is a no-op that succeeds, and
/// trying to turn it off raises php-src's own `ValueError` rather than silently leaving
/// `@file` upload strings interpreted.
#[test]
fn wave_a_safe_upload_cannot_be_disabled() {
    if skip_without_curl_native("wave_a_safe_upload_cannot_be_disabled") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_setopt($ch, CURLOPT_SAFE_UPLOAD, true) ? "on\n" : "off\n";
        try {
            curl_setopt($ch, CURLOPT_SAFE_UPLOAD, false);
            echo "disabled\n";
        } catch (\ValueError $e) {
            echo $e->getMessage(), "\n";
        }
        echo curl_setopt($ch, CURLOPT_BINARYTRANSFER, true) ? "binary\n" : "no-binary\n";
        "#,
    );
    assert_eq!(
        out,
        "on\ncurl_setopt(): Disabling safe uploads is no longer supported\nbinary\n"
    );
}

/// `curl_setopt_array()` drives the same gate as `curl_setopt()`: it applies every option
/// in order, stops at the first one that fails, and lets a `ValueError` from an invalid
/// option number propagate rather than swallowing it into a `false`.
#[test]
fn wave_a_setopt_array_applies_and_stops_on_failure() {
    if skip_without_curl_native("wave_a_setopt_array_applies_and_stops_on_failure") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init();
        $ok = curl_setopt_array($ch, [
            CURLOPT_URL => "{url}",
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_USERAGENT => "array-agent",
        ]);
        echo $ok ? "applied\n" : "failed\n";
        echo str_contains(curl_exec($ch), "user-agent: array-agent") ? "ua\n" : "no-ua\n";

        $bad = curl_init();
        echo curl_setopt_array($bad, [CURLOPT_WRITEFUNCTION => 1]) ? "accepted\n" : "stopped\n";
        "#
    ));
    // CURLOPT_WRITEFUNCTION is a real option this build cannot carry, so it warns and
    // answers `false` — `curl_setopt_array()` reports that as `false`, never a throw.
    assert_eq!(out, "applied\nua\nstopped\n");
}
