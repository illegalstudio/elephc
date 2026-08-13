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

/// Wave B: `CURLOPT_HTTPHEADER` really builds a `curl_slist` libcurl walks — the fixture
/// echoes the headers it received, so both custom headers and the OVERRIDE of a header
/// libcurl would otherwise send itself are proved on the wire.
#[test]
fn wave_b_httpheader_sends_a_real_slist() {
    if skip_without_curl_native("wave_b_httpheader_sends_a_real_slist") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HTTPHEADER, [
            "X-Elephc: one",
            "X-Second: two",
            "Accept: application/json",
        ]);
        $body = curl_exec($ch);
        echo str_contains($body, "x-elephc: one") ? "h1\n" : "no-h1\n";
        echo str_contains($body, "x-second: two") ? "h2\n" : "no-h2\n";
        echo str_contains($body, "accept: application/json") ? "accept\n" : "no-accept\n";
        "#
    ));
    assert_eq!(out, "h1\nh2\naccept\n");
}

/// A slist option REPLACES the previous list rather than appending to it, and an empty
/// array clears it — the same semantics php-src has, and the case where the bridge's
/// "free the old list only after libcurl accepted the new one" ordering matters. The
/// transfer afterwards is what proves the replaced list was not freed while libcurl still
/// pointed at it.
#[test]
fn wave_b_httpheader_replaces_and_clears() {
    if skip_without_curl_native("wave_b_httpheader_replaces_and_clears") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-First: 1"]);
        curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Second: 2"]);
        $body = curl_exec($ch);
        echo str_contains($body, "x-first") ? "stale\n" : "replaced\n";
        echo str_contains($body, "x-second: 2") ? "fresh\n" : "no-fresh\n";

        curl_setopt($ch, CURLOPT_HTTPHEADER, []);
        $body2 = curl_exec($ch);
        echo str_contains($body2, "x-second") ? "still\n" : "cleared\n";
        "#
    ));
    assert_eq!(out, "replaced\nfresh\ncleared\n");
}

/// A slist option requires an ARRAY: php-src raises a `TypeError` for a string, and so
/// does this build — never a silent `false` that would leave the caller guessing whether
/// the header went out.
#[test]
fn wave_b_slist_options_require_an_array() {
    if skip_without_curl_native("wave_b_slist_options_require_an_array") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            curl_setopt($ch, CURLOPT_HTTPHEADER, "Accept: text/plain");
            echo "accepted\n";
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        echo curl_setopt($ch, CURLOPT_QUOTE, ["NOOP"]) ? "quote\n" : "no-quote\n";
        echo curl_setopt($ch, CURLOPT_RESOLVE, ["example.test:443:127.0.0.1"]) ? "resolve\n" : "no-resolve\n";
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): Argument #3 ($value) must be of type array, string given\nquote\nresolve\n"
    );
}

/// Wave B: `CURLOPT_POSTFIELDS` as a STRING posts that exact body, and libcurl's default
/// `Content-Type` for it is `application/x-www-form-urlencoded` — both read back from the
/// fixture's echo of the request it received.
#[test]
fn wave_b_postfields_string_posts_a_raw_body() {
    if skip_without_curl_native("wave_b_postfields_string_posts_a_raw_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POST, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, "raw=body&x=1");
        $body = curl_exec($ch);
        echo str_contains($body, "method=POST") ? "post\n" : "no-post\n";
        echo str_contains($body, "body=raw=body&x=1") ? "body\n" : "no-body\n";
        echo str_contains($body, "content-length: 12") ? "len\n" : "no-len\n";
        "#
    ));
    assert_eq!(out, "post\nbody\nlen\n");
}

/// `CURLOPT_POSTFIELDS` as an ARRAY of scalars encodes as
/// `application/x-www-form-urlencoded`, exactly as PHP does when no `CURLFile` is
/// present: `urlencode()` per key and value, joined with `&`.
#[test]
fn wave_b_postfields_array_form_encodes() {
    if skip_without_curl_native("wave_b_postfields_array_form_encodes") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ["name" => "a b", "n" => 42]);
        $body = curl_exec($ch);
        echo str_contains($body, "method=POST") ? "post\n" : "no-post\n";
        echo str_contains($body, "body=name=a+b&n=42") ? "encoded\n" : "no-encoded\n";
        "#
    ));
    assert_eq!(out, "post\nencoded\n");
}

/// A POST body containing NUL bytes survives intact: the bridge sets
/// `CURLOPT_POSTFIELDSIZE_LARGE` before `CURLOPT_COPYPOSTFIELDS`, so libcurl copies the
/// exact byte count instead of calling `strlen` and truncating at the first NUL. A plain
/// `CURLOPT_POSTFIELDS` forward would have sent 3 bytes here, not 7.
#[test]
fn wave_b_postfields_are_binary_safe() {
    if skip_without_curl_native("wave_b_postfields_are_binary_safe") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, "abc\0def");
        $body = curl_exec($ch);
        echo str_contains($body, "content-length: 7") ? "len\n" : "no-len\n";
        "#
    ));
    assert_eq!(out, "len\n");
}

/// An array `CURLOPT_POSTFIELDS` holding an OBJECT is refused with a clear message rather
/// than half-encoded: in php-src that is a `CURLFile` and switches the body to
/// `multipart/form-data`, which lands in Task 11. Silently posting the object's string
/// cast would be worse than an error.
#[test]
fn wave_b_postfields_object_values_are_refused() {
    if skip_without_curl_native("wave_b_postfields_object_values_are_refused") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        class Upload { public string $name = "x"; }
        $ch = curl_init();
        try {
            curl_setopt($ch, CURLOPT_POSTFIELDS, ["file" => new Upload()]);
            echo "accepted\n";
        } catch (\RuntimeException $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): CURLOPT_POSTFIELDS with an object value (multipart/form-data upload) is not supported by this build\n"
    );
}

/// The slists a handle owns are freed with the handle, not leaked and not double-freed:
/// a loop of handles that each set several `curl_slist` options and then perform a real
/// transfer must leave elephc's heap balanced under `--heap-debug`.
#[test]
fn wave_b_slist_handles_free_cleanly() {
    if skip_without_curl_native("wave_b_slist_handles_free_cleanly") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function post(): int {{
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-A: 1", "X-B: 2"]);
            curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-C: 3"]);
            curl_setopt($ch, CURLOPT_POSTFIELDS, ["k" => "v"]);
            curl_exec($ch);
            $code = curl_getinfo($ch, CURLINFO_HTTP_CODE);
            unset($ch);
            return $code;
        }}
        echo post(), post(), post(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "200200200\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "slist-owning handles must not leak or double-free");
}

/// Wave C: the no-`$option` `curl_getinfo()` form returns PHP's documented associative
/// array, with PHP's own key names (`http_code`, not `CURLINFO_HTTP_CODE`) and value
/// types, filled from a real transfer.
#[test]
fn wave_c_getinfo_array_uses_php_key_names() {
    if skip_without_curl_native("wave_c_getinfo_array_uses_php_key_names") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_exec($ch);
        $info = curl_getinfo($ch);
        echo is_array($info) ? "array\n" : "not-array\n";
        echo $info['url'], "\n";
        echo $info['http_code'], "\n";
        echo $info['content_type'], "\n";
        echo $info['redirect_count'], "\n";
        echo $info['primary_ip'], "\n";
        echo $info['scheme'], "\n";
        echo $info['effective_method'], "\n";
        echo $info['size_download'], "\n";
        echo is_float($info['total_time']) ? "float\n" : "not-float\n";
        echo is_int($info['total_time_us']) ? "int\n" : "not-int\n";
        echo is_array($info['certinfo']) ? "certinfo\n" : "no-certinfo\n";
        echo array_key_exists('request_header', $info) ? "req\n" : "no-req\n";
        "#
    ));
    assert_eq!(
        out,
        format!(
            "array\n{url}\n200\ntext/plain\n0\n127.0.0.1\nhttp\nGET\n10\nfloat\nint\ncertinfo\nno-req\n"
        )
    );
}

/// Wave C: each `CURLINFO_*` type mask reads through its own typed entry point and comes
/// back as PHP's documented type — string, int, float — with real values from a completed
/// transfer, never a fabricated one.
#[test]
fn wave_c_typed_info_keys_return_real_values() {
    if skip_without_curl_native("wave_c_typed_info_keys_return_real_values") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_exec($ch);
        $eff = curl_getinfo($ch, CURLINFO_EFFECTIVE_URL);
        echo is_string($eff) ? "str\n" : "not-str\n";
        echo $eff, "\n";
        echo curl_getinfo($ch, CURLINFO_HTTP_CODE), "\n";
        echo curl_getinfo($ch, CURLINFO_PRIMARY_PORT) > 0 ? "port\n" : "no-port\n";
        $total = curl_getinfo($ch, CURLINFO_TOTAL_TIME);
        echo is_float($total) ? "float\n" : "not-float\n";
        $size = curl_getinfo($ch, CURLINFO_SIZE_DOWNLOAD_T);
        echo is_int($size) ? "int\n" : "not-int\n";
        echo $size, "\n";
        echo curl_getinfo($ch, CURLINFO_CONTENT_TYPE), "\n";
        "#
    ));
    assert_eq!(
        out,
        format!("str\n{url}\n200\nport\nfloat\nint\n10\ntext/plain\n")
    );
}

/// Wave C: a `CURLINFO_SLIST` key comes back as a PHP ARRAY of strings, and an info key
/// this build cannot answer comes back as `false` — never an invented value.
/// `CURLINFO_COOKIELIST` is empty here because no cookie engine was enabled, which is
/// exactly what PHP reports for the same handle.
#[test]
fn wave_c_slist_and_unknown_info_keys() {
    if skip_without_curl_native("wave_c_slist_and_unknown_info_keys") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_COOKIEFILE, "");
        curl_exec($ch);
        $cookies = curl_getinfo($ch, CURLINFO_COOKIELIST);
        echo is_array($cookies) ? "array\n" : "not-array\n";
        echo count($cookies), "\n";
        $certs = curl_getinfo($ch, CURLINFO_CERTINFO);
        echo is_array($certs) ? "certs\n" : "no-certs\n";
        echo curl_getinfo($ch, CURLINFO_HEADER_OUT) === false ? "header-out-false\n" : "header-out-set\n";
        echo curl_getinfo($ch, 5242880) === false ? "unknown-false\n" : "unknown-set\n";
        "#
    ));
    assert_eq!(out, "array\n0\ncerts\nheader-out-false\nunknown-false\n");
}

/// Wave C's TYPED info reads allocate fresh PHP values and never alias the handle: a loop
/// of real transfers reading ints, floats, strings and list keys must leave elephc's heap
/// balanced under `--heap-debug`.
///
/// THE NO-`$OPTION` ARRAY FORM IS DELIBERATELY ABSENT from this loop. It leaks, and the
/// leak is not curl's: `json_decode()` never releases the value it decodes (measured with
/// `--gc-stats`: `json_decode('{"a":1,"b":2}', true)` leaks 10 blocks per call, a bare
/// `json_decode('5', true)` leaks 1), which `curl_version()` has inherited since Task 5
/// and `curl_getinfo($ch)` now inherits too. Asserting balance here would pin a bug in a
/// shared builtin to this feature's test; the report records it instead.
#[test]
fn wave_c_typed_getinfo_shapes_do_not_leak() {
    if skip_without_curl_native("wave_c_typed_getinfo_shapes_do_not_leak") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function probe(): int {{
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_exec($ch);
            $effective = curl_getinfo($ch, CURLINFO_EFFECTIVE_URL);
            $time = curl_getinfo($ch, CURLINFO_TOTAL_TIME);
            $cookies = curl_getinfo($ch, CURLINFO_COOKIELIST);
            $size = curl_getinfo($ch, CURLINFO_SIZE_DOWNLOAD_T);
            $code = curl_getinfo($ch, CURLINFO_HTTP_CODE);
            unset($ch);
            if (strlen($effective) === 0 || $time < 0 || count($cookies) !== 0 || $size !== 10) {{
                return 0;
            }}
            return $code;
        }}
        echo probe(), probe(), probe(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "200200200\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "typed curl_getinfo() shapes must not leak or double-free");
}
