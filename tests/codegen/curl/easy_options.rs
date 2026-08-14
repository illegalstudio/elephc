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
        // php-src `ZVAL_COPY`s whatever it is handed here, so an ARRAY is legal and reads
        // back verbatim. The scalar type guard the other options need must not apply.
        echo curl_setopt($ch, CURLOPT_PRIVATE, ["a", "b"]) ? "array-ok\n" : "array-refused\n";
        $stored = curl_getinfo($ch, CURLINFO_PRIVATE);
        echo is_array($stored) ? "array\n" : "not-array\n";
        echo $stored[1], "\n";
        echo curl_setopt($ch, CURLOPT_PRIVATE, null) ? "null-ok\n" : "null-refused\n";
        echo curl_getinfo($ch, CURLINFO_PRIVATE) === null ? "null\n" : "not-null\n";
        "#,
    );
    assert_eq!(
        out,
        "unset\nrequest-42\narray-ok\narray\nb\nnull-ok\nnull\n"
    );
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
        echo curl_setopt_array($bad, [CURLOPT_FNMATCH_FUNCTION => 1]) ? "accepted\n" : "stopped\n";
        "#
    ));
    // CURLOPT_FNMATCH_FUNCTION is a real option this build cannot carry, so it warns and
    // answers `false` — `curl_setopt_array()` reports that as `false`, never a throw.
    // (It used to be CURLOPT_WRITEFUNCTION here; Task 12 implements that one, so the
    // rejection example moved to a callback option still in the second wave.)
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

/// TASK 11: `CURLOPT_POSTFIELDS` as an ARRAY of scalars now posts REAL
/// `multipart/form-data` — one part per key/value pair — exactly as php-src does whether
/// or not the array contains a `CURLFile`. This REPLACES the Task 8 stopgap that
/// urlencoded a scalar array (a documented divergence, now gone): `wave_b_postfields_...`
/// keeps its name for `git blame` continuity even though the assertion is now Task 11's.
#[test]
fn wave_b_postfields_array_form_encodes() {
    if skip_without_curl_native("wave_b_postfields_array_form_encodes") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/multipart");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_POSTFIELDS, ["name" => "a b", "n" => 42]);
        $body = curl_exec($ch);
        echo str_contains($body, "method=POST") ? "post\n" : "no-post\n";
        echo str_contains($body, "content-type=multipart/form-data") ? "multipart\n" : "no-multipart\n";
        echo str_contains($body, "parts=2") ? "parts\n" : "no-parts\n";
        echo str_contains($body, "part[0].name=name") ? "name0\n" : "no-name0\n";
        echo str_contains($body, "part[0].body=a b") ? "body0\n" : "no-body0\n";
        echo str_contains($body, "part[1].name=n") ? "name1\n" : "no-name1\n";
        echo str_contains($body, "part[1].body=42") ? "body1\n" : "no-body1\n";
        "#
    ));
    assert_eq!(
        out,
        "post\nmultipart\nparts\nname0\nbody0\nname1\nbody1\n"
    );
}

/// PUNCH-LIST ITEM 16: `CURLOPT_POSTFIELDS => []` posts an EMPTY STRING BODY, not an
/// empty multipart. php-src special-cases the empty array before it builds any mime
/// structure, and the difference is visible on the wire: measured on PHP 8.4.20 against a
/// local echo server, `[]` sends `Content-Type: application/x-www-form-urlencoded` with a
/// zero-length body — identical to `CURLOPT_POSTFIELDS => ""` — whereas a built-but-empty
/// `curl_mime` sends `multipart/form-data` with a boundary. The fixture's `/echo` route
/// dumps every request header plus the body, so both halves are checked here.
#[test]
fn postfields_empty_array_posts_an_empty_urlencoded_body() {
    if skip_without_curl_native("postfields_empty_array_posts_an_empty_urlencoded_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        function post($url, $fields) {{
            $ch = curl_init($url);
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_POSTFIELDS, $fields);
            $body = curl_exec($ch);
            echo str_contains($body, "method=POST") ? "post " : "no-post ";
            echo str_contains($body, "content-type: application/x-www-form-urlencoded") ? "urlencoded " : "not-urlencoded ";
            echo str_contains($body, "multipart") || str_contains($body, "boundary") ? "multipart " : "no-multipart ";
            echo str_ends_with($body, "body=\n") ? "empty-body\n" : "some-body\n";
        }}
        post("{url}", []);
        post("{url}", "");
        "#
    ));
    // Both forms must answer identically — that is the whole claim.
    assert_eq!(
        out,
        "post urlencoded no-multipart empty-body\npost urlencoded no-multipart empty-body\n"
    );
}

/// PUNCH-LIST ITEM 2: a copy starts with a CLEAN transfer record. `curl_errno()` /
/// `curl_error()` / `curl_multi_getcontent()` on a freshly copied handle report the copy's
/// own (empty) history, never the source's last transfer — measured on PHP 8.4.20, where a
/// copy of a handle that had just failed with `CURLE_COULDNT_CONNECT` answers `0` / `""` /
/// `""`. `CURLOPT_RETURNTRANSFER` is an OPTION and does travel, which the last line pins so
/// the fix cannot regress into "the copy streams to stdout instead".
#[test]
fn copy_handle_starts_with_a_clean_error_and_body() {
    if skip_without_curl_native("copy_handle_starts_with_a_clean_error_and_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // 1. after a SUCCESSFUL transfer, the captured body does not travel.
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $body = curl_exec($ch);
        echo $body === "hello-curl" ? "orig-body\n" : "orig-wrong\n";
        echo curl_multi_getcontent($ch) === "hello-curl" ? "orig-content\n" : "orig-no-content\n";
        $copy = curl_copy_handle($ch);
        echo curl_multi_getcontent($copy) === "" ? "copy-empty\n" : "copy-inherited\n";
        echo curl_errno($copy), "\n";
        echo curl_error($copy) === "" ? "copy-no-error\n" : "copy-error\n";

        // 2. after a FAILED transfer, errno/error do not travel either.
        $bad = curl_init("xyzzy://not-a-protocol");
        curl_setopt($bad, CURLOPT_RETURNTRANSFER, true);
        $failed = curl_exec($bad);
        echo $failed === false ? "failed\n" : "unexpected-success\n";
        echo curl_errno($bad) !== 0 ? "orig-errno\n" : "orig-no-errno\n";
        echo curl_error($bad) !== "" ? "orig-message\n" : "orig-no-message\n";
        $badCopy = curl_copy_handle($bad);
        echo curl_errno($badCopy), "\n";
        echo curl_error($badCopy) === "" ? "clean\n" : "dirty\n";

        // 3. RETURNTRANSFER itself is an option and DID travel: the copy captures.
        echo curl_exec($copy) === "hello-curl" ? "copy-captures\n" : "copy-streams\n";
        "#
    ));
    assert_eq!(
        out,
        "orig-body\norig-content\ncopy-empty\n0\ncopy-no-error\n\
         failed\norig-errno\norig-message\n0\nclean\ncopy-captures\n"
    );
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

/// TASK 11: an array `CURLOPT_POSTFIELDS` holding a `CURLFile`/`CURLStringFile` now
/// UPLOADS it as a real mime part (see `multipart.rs`) instead of being refused — this
/// test now covers the one object shape that is STILL refused: any OTHER object class.
/// Real php-src would attempt `(string) $value` here (accepting a `Stringable` object,
/// raising a catchable `\Error` otherwise); elephc's own object-to-string cast for a class
/// with no `__toString()` is an UNCATCHABLE process exit, so `__elephc_curl_build_multipart`
/// refuses any non-`CURLFile`/`CURLStringFile` object explicitly, with a catchable
/// `\TypeError`, before ever reaching that cast — see `src/curl_prelude.rs`'s
/// `__elephc_curl_build_multipart` doc comment for the full reasoning.
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
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "curl_setopt(): CURLOPT_POSTFIELDS array value must be of type string|int|float|bool|CURLFile|CURLStringFile, Upload given\n"
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

/// A PHP ARRAY IS ORDERED, so `curl_getinfo()`'s KEY ORDER is part of its shape. It is
/// php-src's field order, measured with `array_keys(curl_getinfo($ch))` on PHP 8.4.20:
/// `url, content_type, http_code, header_size, …, effective_method, capath, cainfo`. The
/// bridge encodes the array as JSON (`crates/elephc-curl/src/info.rs`) and `serde_json`'s
/// `preserve_order` feature is what carries the insertion order through the blob — without
/// it the decoded array came back byte-sorted (`appconnect_time_us` first), which no
/// `foreach` over a real `ext/curl` result ever produces.
///
/// `posttransfer_time_us` sits between `starttransfer_time_us` and `total_time_us` in
/// php's list and is absent here — a MISSING KEY, tracked separately; this fixture pins the
/// order of the keys this build does report.
///
/// IT ALSO PINS THAT EACH `*_us` KEY REALLY IS ITS OWN TIMER. The first run of this fixture
/// caught five scrambled `CURLINFO_*_T` numbers in `crates/elephc-curl/src/info.rs`:
/// `redirect_time_us`/`starttransfer_time_us` were missing from the array entirely (their
/// numbers were not `CURLINFO_OFF_T` fields), and `namelookup_time_us`/`connect_time_us`/
/// `pretransfer_time_us` were silently reporting other fields. Comparing every `*_us`
/// against its `double` counterpart is what makes that class of mistake loud.
#[test]
fn getinfo_array_keys_are_in_php_s_order() {
    if skip_without_curl_native("getinfo_array_keys_are_in_php_s_order") {
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
        echo implode(",", array_keys($info)), "\n";
        // Each microsecond timer must be its own double timer, to the microsecond: a
        // swapped CURLINFO number would report a different (usually smaller or zero) field.
        $pairs = [
            "total_time", "namelookup_time", "connect_time", "pretransfer_time",
            "starttransfer_time", "redirect_time",
        ];
        foreach ($pairs as $name) {{
            $micros = (int) round($info[$name] * 1000000.0);
            $reported = $info[$name . "_us"];
            $delta = $micros > $reported ? $micros - $reported : $reported - $micros;
            echo $name, $delta <= 1 ? "=ok " : "=MISMATCH({{$micros}} vs {{$reported}}) ";
        }}
        echo "\n";
        "#
    ));
    assert_eq!(
        out,
        "url,content_type,http_code,header_size,request_size,filetime,ssl_verify_result,\
         redirect_count,total_time,namelookup_time,connect_time,pretransfer_time,size_upload,\
         size_download,speed_download,speed_upload,download_content_length,\
         upload_content_length,starttransfer_time,redirect_time,redirect_url,primary_ip,\
         certinfo,primary_port,local_ip,local_port,http_version,protocol,ssl_verifyresult,\
         scheme,appconnect_time_us,connect_time_us,namelookup_time_us,pretransfer_time_us,\
         redirect_time_us,starttransfer_time_us,total_time_us,effective_method,capath,cainfo\n\
         total_time=ok namelookup_time=ok connect_time=ok pretransfer_time=ok \
         starttransfer_time=ok redirect_time=ok \n"
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

/// Wave D: `curl_reset()` really puts libcurl's options back to default — a handle whose
/// URL, headers and RETURNTRANSFER were set stops carrying any of them — while staying
/// the SAME `CurlHandle` object, and stays usable for a fresh transfer afterwards.
#[test]
fn wave_d_reset_clears_options_and_php_state() {
    if skip_without_curl_native("wave_d_reset_clears_options_and_php_state") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let echo = server.url("/echo");
    let hello = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{echo}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Before: 1"]);
        curl_setopt($ch, CURLOPT_PRIVATE, "before");
        $first = curl_exec($ch);
        echo str_contains($first, "x-before: 1") ? "before\n" : "no-before\n";

        curl_reset($ch);
        echo get_class($ch), "\n";
        echo curl_getinfo($ch, CURLINFO_PRIVATE) === false ? "private-cleared\n" : "private-kept\n";

        curl_setopt($ch, CURLOPT_URL, "{echo}");
        $second = curl_exec($ch);
        echo $second === true ? "streamed\n" : "captured\n";
        echo curl_getinfo($ch, CURLINFO_HTTP_CODE), "\n";

        curl_setopt($ch, CURLOPT_URL, "{hello}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        echo curl_exec($ch), "\n";
        "#
    ));
    assert!(
        out.starts_with("before\nCurlHandle\nprivate-cleared\n"),
        "{out}"
    );
    assert!(out.ends_with("streamed\n200\nhello-curl\n"), "{out}");
}

/// Wave D: `curl_copy_handle()` duplicates BOTH layers — libcurl's options and the
/// PHP-layer state. The copy is a distinct `CurlHandle` that carries the original's URL,
/// headers, RETURNTRANSFER capture (so `curl_exec()` keeps its string return shape) and
/// `CURLOPT_PRIVATE`, and changing the copy does not disturb the original.
#[test]
fn wave_d_copy_handle_duplicates_both_layers() {
    if skip_without_curl_native("wave_d_copy_handle_duplicates_both_layers") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_USERAGENT, "original-agent");
        curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-Copied: yes"]);
        curl_setopt($ch, CURLOPT_PRIVATE, "tag-1");

        $copy = curl_copy_handle($ch);
        echo get_class($copy), "\n";
        echo ($copy === $ch) ? "same\n" : "distinct\n";
        echo curl_getinfo($copy, CURLINFO_PRIVATE), "\n";

        $body = curl_exec($copy);
        echo is_string($body) ? "captured\n" : "streamed\n";
        echo str_contains($body, "user-agent: original-agent") ? "ua\n" : "no-ua\n";
        echo str_contains($body, "x-copied: yes") ? "header\n" : "no-header\n";

        curl_setopt($copy, CURLOPT_USERAGENT, "copy-agent");
        $original = curl_exec($ch);
        echo str_contains($original, "user-agent: original-agent") ? "unchanged\n" : "changed\n";
        "#
    ));
    assert_eq!(
        out,
        "CurlHandle\ndistinct\ntag-1\ncaptured\nua\nheader\nunchanged\n"
    );
}

/// REGRESSION: a copied handle must own its own `curl_slist` options, so it stays usable
/// after everything that frees the ORIGINAL's lists — the original being destroyed, the
/// original being reset, and the original's list option being replaced.
///
/// `curl_easy_duphandle` does NOT duplicate slist options: `dupset` (libcurl 8.21.0,
/// `lib/easy.c`) shallow-copies `src->set` and then re-duplicates only strings, blobs,
/// `COPYPOSTFIELDS` and mime, so every `struct curl_slist *` is SHARED. A copy that
/// inherited the pointer read freed memory the moment this bridge released the source's
/// list — a use-after-free, not a lost header. Each of the three transfers below is one of
/// the three ways to reach it, and all three assert the headers really arrived on the wire
/// (a copy whose option had merely been cleared would still transfer, just without them).
#[test]
fn wave_d_copied_handle_owns_its_slists() {
    if skip_without_curl_native("wave_d_copied_handle_owns_its_slists") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        // 1. the original is DESTROYED before the copy transfers.
        $a = curl_init("{url}");
        curl_setopt($a, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($a, CURLOPT_HTTPHEADER, ["X-Owned: one", "X-Second: two"]);
        $b = curl_copy_handle($a);
        unset($a);
        $body = curl_exec($b);
        echo $body === false ? "failed" : "ok", "\n";
        echo str_contains($body, "x-owned: one") ? "h1\n" : "no-h1\n";
        echo str_contains($body, "x-second: two") ? "h2\n" : "no-h2\n";

        // 2. the original is RESET before the copy transfers.
        $c = curl_init("{url}");
        curl_setopt($c, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($c, CURLOPT_HTTPHEADER, ["X-Reset: three"]);
        $d = curl_copy_handle($c);
        curl_reset($c);
        $body2 = curl_exec($d);
        echo str_contains($body2, "x-reset: three") ? "h3\n" : "no-h3\n";

        // 3. the original's list option is REPLACED before the copy transfers, which frees
        //    the list the copy would otherwise still be pointing at.
        $e = curl_init("{url}");
        curl_setopt($e, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($e, CURLOPT_HTTPHEADER, ["X-Old: four"]);
        $f = curl_copy_handle($e);
        curl_setopt($e, CURLOPT_HTTPHEADER, ["X-New: five"]);
        $body3 = curl_exec($f);
        echo str_contains($body3, "x-old: four") ? "h4\n" : "no-h4\n";
        echo str_contains($body3, "x-new") ? "leaked-new\n" : "isolated\n";
        "#
    ));
    assert_eq!(out, "ok\nh1\nh2\nh3\nh4\nisolated\n");
}

/// Wave D: `curl_escape()` / `curl_unescape()` are libcurl's own percent-encoders, not
/// PHP's `urlencode()` — a space becomes `%20`, never `+` — and they round-trip.
#[test]
fn wave_d_escape_and_unescape_round_trip() {
    if skip_without_curl_native("wave_d_escape_and_unescape_round_trip") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        $escaped = curl_escape($ch, "a b&c=d/42");
        echo $escaped, "\n";
        echo curl_unescape($ch, $escaped), "\n";
        echo curl_unescape($ch, "%48%65llo%21"), "\n";
        echo curl_escape($ch, ""), "|\n";
        "#,
    );
    assert_eq!(out, "a%20b%26c%3Dd%2F42\na b&c=d/42\nHello!\n|\n");
}

/// Wave D: `curl_pause()` answers a `CURLcode` (`CURLE_OK` on an idle handle),
/// `curl_upkeep()` answers a bool, and `curl_strerror()` reports libcurl's own text for a
/// code — including a recognizable message for a code libcurl does not know.
#[test]
fn wave_d_pause_upkeep_and_strerror() {
    if skip_without_curl_native("wave_d_pause_upkeep_and_strerror") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        $paused = curl_pause($ch, CURLPAUSE_ALL);
        echo is_int($paused) ? "int\n" : "not-int\n";
        echo curl_pause($ch, CURLPAUSE_CONT), "\n";
        echo is_bool(curl_upkeep($ch)) ? "bool\n" : "not-bool\n";
        echo curl_strerror(CURLE_OK), "\n";
        echo curl_strerror(CURLE_UNSUPPORTED_PROTOCOL), "\n";
        echo strlen(curl_strerror(9999)) > 0 ? "unknown\n" : "empty\n";
        "#,
    );
    assert_eq!(
        out,
        // 43 is CURLE_BAD_FUNCTION_ARGUMENT: libcurl's own answer for pausing a handle
        // that has no connection, which an idle handle never does (lib/easy.c refuses
        // before touching the pause state). php-src reports the same code.
        "int\n43\nbool\nNo error\nUnsupported protocol\nunknown\n"
    );
}

/// Wave D's handle-producing and string-producing calls keep the heap balanced: a loop of
/// copies, resets and escapes over real transfers must free every handle it mints exactly
/// once — a `curl_copy_handle()` mis-categorized as aliasing its argument would keep the
/// SOURCE handle (and its socket) alive forever, and a double-free would show up here too.
#[test]
fn wave_d_lifecycle_calls_do_not_leak() {
    if skip_without_curl_native("wave_d_lifecycle_calls_do_not_leak") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let output = compile_and_run_with_gc_stats(&format!(
        r#"<?php
        function cycle(): int {{
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
            curl_setopt($ch, CURLOPT_HTTPHEADER, ["X-A: 1"]);
            $copy = curl_copy_handle($ch);
            $body = curl_exec($copy);
            $escaped = curl_escape($copy, "a b");
            $message = curl_strerror(curl_errno($copy));
            $code = curl_getinfo($copy, CURLINFO_HTTP_CODE);
            curl_reset($ch);
            unset($copy);
            unset($ch);
            if ($body !== "hello-curl" || $escaped !== "a%20b" || strlen($message) === 0) {{
                return 0;
            }}
            return $code;
        }}
        echo cycle(), cycle(), cycle(), "\n";
        "#
    ));
    assert_eq!(output.stdout, "200200200\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "Wave D lifecycle calls must not leak or double-free");
}

/// A string info field libcurl answers with a NULL `char *` is `false` in the typed
/// `curl_getinfo($ch, CURLINFO_*)` form and `null` under `content_type` in the array form
/// — php-src's two answers, and neither is `""`.
///
/// `/notype` responds without a `Content-Type` header, which is the ordinary way to reach
/// a NULL info pointer. The `/hello` half of the test is what keeps the distinction
/// meaningful: the same two reads carry a real value there, so a build that answered
/// `false`/`null` unconditionally would fail here too.
#[test]
fn wave_c_null_string_info_is_false_and_null_not_empty() {
    if skip_without_curl_native("wave_c_null_string_info_is_false_and_null_not_empty") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let typeless = server.url("/notype");
    let typed = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $none = curl_init("{typeless}");
        curl_setopt($none, CURLOPT_RETURNTRANSFER, true);
        curl_exec($none);
        $value = curl_getinfo($none, CURLINFO_CONTENT_TYPE);
        echo $value === false ? "typed-false\n" : ($value === "" ? "typed-empty\n" : "typed-value\n");
        $info = curl_getinfo($none);
        echo array_key_exists('content_type', $info) ? "key\n" : "no-key\n";
        echo $info['content_type'] === null ? "array-null\n" : "array-not-null\n";

        $some = curl_init("{typed}");
        curl_setopt($some, CURLOPT_RETURNTRANSFER, true);
        curl_exec($some);
        echo curl_getinfo($some, CURLINFO_CONTENT_TYPE), "\n";
        echo curl_getinfo($some)['content_type'], "\n";
        "#
    ));
    assert_eq!(
        out,
        "typed-false\nkey\narray-null\ntext/plain\ntext/plain\n"
    );
}
