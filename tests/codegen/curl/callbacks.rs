//! Purpose:
//! End-to-end fixtures for the six first-wave `ext/curl` callback options, where a C
//! callback inside `curl_easy_perform()` re-enters compiled PHP: `CURLOPT_WRITEFUNCTION`,
//! `CURLOPT_HEADERFUNCTION`, `CURLOPT_READFUNCTION`, `CURLOPT_PROGRESSFUNCTION`,
//! `CURLOPT_XFERINFOFUNCTION`, and `CURLOPT_DEBUGFUNCTION`.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl::callbacks` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here was MEASURED against the host PHP 8.4.20 + ext/curl before it
//!   was written down (see `.superpowers/sdd/php-curl-family/task-12-report.md`), not
//!   inferred from the manual. The surprising ones: `curl_setopt(WRITEFUNCTION, …)`
//!   OVERRIDES `RETURNTRANSFER` (and vice versa — the later call wins, because php-src
//!   keeps a single write-mode enum); a short `WRITEFUNCTION`/`HEADERFUNCTION` return
//!   aborts with `CURLE_WRITE_ERROR` (23); `CURLOPT_PROGRESSFUNCTION` does NOT
//!   auto-enable progress reporting (`CURLOPT_NOPROGRESS` must be turned off by hand);
//!   a `READFUNCTION` return longer than `$length` is TRUNCATED, never an error.
//! - `use (&$buf)` per-variable by-reference capture is the reliable aliasing mechanism
//!   for accumulating callback output.
//! - The fixture server drains exactly `Content-Length` bytes, so the upload fixtures set
//!   `CURLOPT_INFILESIZE` and libcurl sends a sized `PUT` instead of a chunked one.

use super::http_fixture::LocalHttpServer;
use crate::support::*;

/// `CURLOPT_WRITEFUNCTION` receives every body chunk as a PHP string and `curl_exec()`
/// answers `true` rather than the body — the callback, not the caller, owns the bytes.
#[test]
fn curl_writefunction_accumulates_the_body() {
    if skip_without_curl_native("curl_writefunction_accumulates_the_body") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $buf = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$buf): int {{
            $buf .= $data;
            return strlen($data);
        }});
        $r = curl_exec($ch);
        echo $buf;
        echo "|", $r === true ? "T" : "F";
        echo "|", curl_errno($ch);
        "#
    ));
    assert_eq!(out, "hello-curl|T|0");
}

/// A `CURLOPT_WRITEFUNCTION` that returns fewer bytes than it was given aborts the
/// transfer with `CURLE_WRITE_ERROR` (23), and `curl_exec()` answers `false`.
#[test]
fn curl_writefunction_short_return_aborts_with_write_error() {
    if skip_without_curl_native("curl_writefunction_short_return_aborts_with_write_error") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            return 1;
        }});
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X";
        echo "|", curl_errno($ch);
        echo "|", strlen(curl_error($ch)) > 0 ? "M" : "N";
        "#
    ));
    assert_eq!(out, "F|23|M");
}

/// PHP reads a write callback's return with `zval_get_long`, so `true` becomes `1` and
/// `null` becomes `0` — both mismatch the chunk length and abort the transfer. Measured
/// against PHP 8.4.20, where both answer `curl_errno() === 23`.
#[test]
fn curl_writefunction_non_integer_returns_follow_php_int_cast() {
    if skip_without_curl_native("curl_writefunction_non_integer_returns_follow_php_int_cast") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): mixed {{
            return true;
        }});
        curl_exec($ch);
        echo curl_errno($ch);
        $ch2 = curl_init("{url}");
        // A numeric STRING is a valid length in PHP: "10" casts to 10, which is exactly
        // strlen("hello-curl"), so this transfer succeeds.
        curl_setopt($ch2, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): mixed {{
            return "10";
        }});
        curl_exec($ch2);
        echo "|", curl_errno($ch2);
        "#
    ));
    assert_eq!(out, "23|0");
}

/// `CURLOPT_HEADERFUNCTION` gets one call per response header line — the status line
/// first, each field after it, and a final bare CRLF — while the body still follows the
/// handle's own write mode.
#[test]
fn curl_headerfunction_receives_the_status_line_and_headers() {
    if skip_without_curl_native("curl_headerfunction_receives_the_status_line_and_headers") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // Accumulated into a STRING, not an array: `$arr[] = $s` inside a closure with
        // `use (&$arr)` drops the value on this build (a pre-existing codegen bug with
        // nothing to do with curl — reproduced with no curl call in sight). Per-variable
        // `use (&$var)` on a scalar is the reliable aliasing mechanism.
        $lines = '';
        $count = 0;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HEADERFUNCTION, function (CurlHandle $ch, string $header) use (&$lines, &$count): int {{
            $lines .= trim($header) . "\n";
            $count = $count + 1;
            return strlen($header);
        }});
        $body = curl_exec($ch);
        echo $body;
        echo "|", strpos($lines, "HTTP/1.0 200 OK") === 0 ? "STATUS" : "X";
        echo "|", strpos($lines, "Content-Type:") !== false ? "CT" : "X";
        echo "|", $count >= 3 ? "N" : "F";
        "#
    ));
    assert_eq!(out, "hello-curl|STATUS|CT|N");
}

/// A short `CURLOPT_HEADERFUNCTION` return aborts exactly like the write callback's.
#[test]
fn curl_headerfunction_short_return_aborts() {
    if skip_without_curl_native("curl_headerfunction_short_return_aborts") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_HEADERFUNCTION, function (CurlHandle $ch, string $header): int {{
            return 1;
        }});
        $r = curl_exec($ch);
        echo $r === false ? "F" : "X", "|", curl_errno($ch);
        "#
    ));
    assert_eq!(out, "F|23");
}

/// `CURLOPT_READFUNCTION` drives an upload: libcurl asks for bytes, the callable answers
/// with strings, and an empty string is end-of-data. `$fd` is `null` because this build
/// carries no `CURLOPT_INFILE` stream — which is also what php-src passes when none was
/// set.
#[test]
fn curl_readfunction_drives_an_upload() {
    if skip_without_curl_native("curl_readfunction_drives_an_upload") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $chunks = ["abc", "defg"];
        $index = 0;
        $sawNullFd = 1;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        curl_setopt($ch, CURLOPT_INFILESIZE, 7);
        curl_setopt($ch, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length) use (&$index, $chunks, &$sawNullFd): string {{
            if ($fd !== null) {{
                $sawNullFd = 0;
            }}
            if ($index >= 2) {{
                return "";
            }}
            $piece = $chunks[$index];
            $index = $index + 1;
            return $piece;
        }});
        $body = curl_exec($ch);
        echo strpos($body, "method=PUT") === 0 ? "PUT" : "X";
        echo "|", strpos($body, "body=abcdefg") !== false ? "OK" : "BAD";
        echo "|", curl_errno($ch);
        echo "|", $sawNullFd;
        "#
    ));
    assert_eq!(out, "PUT|OK|0|1");
}

/// A `CURLOPT_READFUNCTION` return longer than `$length` is TRUNCATED, not an error —
/// php-src copies `MIN(size * nmemb, strlen($ret))` bytes (measured on PHP 8.4.20, which
/// uploads the first `CURLOPT_INFILESIZE` bytes and reports `curl_errno() === 0`).
#[test]
fn curl_readfunction_truncates_an_over_long_return() {
    if skip_without_curl_native("curl_readfunction_truncates_an_over_long_return") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $done = false;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        curl_setopt($ch, CURLOPT_INFILESIZE, 5);
        curl_setopt($ch, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length) use (&$done): string {{
            if ($done) {{
                return "";
            }}
            $done = true;
            return str_repeat("Z", $length + 100);
        }});
        $body = curl_exec($ch);
        echo strpos($body, "body=ZZZZZ") !== false ? "OK" : "BAD";
        echo "|", curl_errno($ch);
        "#
    ));
    assert_eq!(out, "OK|0");
}

/// A `CURLOPT_READFUNCTION` that answers with something other than a string is
/// end-of-data, not an error — php-src only copies bytes when the return `IS_STRING`.
#[test]
fn curl_readfunction_non_string_return_is_end_of_data() {
    if skip_without_curl_native("curl_readfunction_non_string_return_is_end_of_data") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        curl_setopt($ch, CURLOPT_INFILESIZE, 0);
        curl_setopt($ch, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length): mixed {{
            return 0;
        }});
        $body = curl_exec($ch);
        echo strpos($body, "body=") !== false ? "OK" : "BAD";
        echo "|", curl_errno($ch);
        "#
    ));
    assert_eq!(out, "OK|0");
}

/// `CURLOPT_PROGRESSFUNCTION` needs `CURLOPT_NOPROGRESS` turned off by hand — php-src
/// does NOT enable progress reporting for you (measured on PHP 8.4.20: with
/// `CURLOPT_NOPROGRESS` left at its default the callback never fires). Returning nonzero
/// aborts the transfer with `CURLE_ABORTED_BY_CALLBACK` (42).
#[test]
fn curl_progressfunction_needs_noprogress_off_and_can_abort() {
    if skip_without_curl_native("curl_progressfunction_needs_noprogress_off_and_can_abort") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // Default CURLOPT_NOPROGRESS: never called, transfer succeeds.
        $silent = 0;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_PROGRESSFUNCTION, function (CurlHandle $ch, int $dltotal, int $dlnow, int $ultotal, int $ulnow) use (&$silent): int {{
            $silent = $silent + 1;
            return 0;
        }});
        $r = curl_exec($ch);
        echo $r === "hello-curl" ? "BODY" : "X", "|", $silent;

        // CURLOPT_NOPROGRESS off: called, and a nonzero return aborts.
        $fired = 0;
        $ch2 = curl_init("{url}");
        curl_setopt($ch2, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch2, CURLOPT_NOPROGRESS, false);
        curl_setopt($ch2, CURLOPT_PROGRESSFUNCTION, function (CurlHandle $ch, int $dltotal, int $dlnow, int $ultotal, int $ulnow) use (&$fired): int {{
            $fired = $fired + 1;
            return 1;
        }});
        $r2 = curl_exec($ch2);
        echo "|", $r2 === false ? "F" : "X", "|", curl_errno($ch2), "|", $fired > 0 ? "CALLED" : "SILENT";
        "#
    ));
    assert_eq!(out, "BODY|0|F|42|CALLED");
}

/// `CURLOPT_XFERINFOFUNCTION` has the same PHP-visible shape as the progress callback —
/// four `int` counters — and takes precedence when both are installed, matching libcurl's
/// own rule and php-src's separate handler records.
#[test]
fn curl_xferinfofunction_fires_and_wins_over_progress() {
    if skip_without_curl_native("curl_xferinfofunction_fires_and_wins_over_progress") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $xfer = 0;
        $progress = 0;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_NOPROGRESS, false);
        curl_setopt($ch, CURLOPT_XFERINFOFUNCTION, function (CurlHandle $ch, int $dltotal, int $dlnow, int $ultotal, int $ulnow) use (&$xfer): int {{
            $xfer = $xfer + 1;
            return 0;
        }});
        curl_setopt($ch, CURLOPT_PROGRESSFUNCTION, function (CurlHandle $ch, int $dltotal, int $dlnow, int $ultotal, int $ulnow) use (&$progress): int {{
            $progress = $progress + 1;
            return 0;
        }});
        $r = curl_exec($ch);
        echo $r === "hello-curl" ? "BODY" : "X";
        echo "|", $xfer > 0 ? "XFER" : "NONE";
        echo "|", $progress;
        "#
    ));
    assert_eq!(out, "BODY|XFER|0");
}

/// `CURLOPT_DEBUGFUNCTION` fires only while `CURLOPT_VERBOSE` is on — php-src does not
/// turn it on for you — and receives `CURLINFO_*` type codes with the raw text.
#[test]
fn curl_debugfunction_requires_verbose_and_reports_info_types() {
    if skip_without_curl_native("curl_debugfunction_requires_verbose_and_reports_info_types") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // No CURLOPT_VERBOSE: libcurl never calls the debug callback.
        $quiet = 0;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_DEBUGFUNCTION, function (CurlHandle $ch, int $type, string $data) use (&$quiet): int {{
            $quiet = $quiet + 1;
            return 0;
        }});
        curl_exec($ch);
        echo $quiet;

        $texts = 0;
        $headersOut = 0;
        $ch2 = curl_init("{url}");
        curl_setopt($ch2, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch2, CURLOPT_VERBOSE, true);
        curl_setopt($ch2, CURLOPT_DEBUGFUNCTION, function (CurlHandle $ch, int $type, string $data) use (&$texts, &$headersOut): int {{
            if ($type === CURLINFO_TEXT) {{
                $texts = $texts + 1;
            }}
            if ($type === CURLINFO_HEADER_OUT && strpos($data, "GET ") === 0) {{
                $headersOut = $headersOut + 1;
            }}
            return 0;
        }});
        $r = curl_exec($ch2);
        echo "|", $r === "hello-curl" ? "BODY" : "X";
        echo "|", $texts > 0 ? "TEXT" : "NONE";
        echo "|", $headersOut > 0 ? "REQ" : "NONE";
        "#
    ));
    assert_eq!(out, "0|BODY|TEXT|REQ");
}

/// The `$ch` a callback receives is the SAME `CurlHandle` object the caller holds, not a
/// fresh wrapper around the same native handle — `===` on objects is identity, and php
/// passes `ch->self`.
#[test]
fn curl_callback_receives_the_identical_handle_object() {
    if skip_without_curl_native("curl_callback_receives_the_identical_handle_object") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $identical = 0;
        $isHandle = 0;
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $received, string $data) use (&$identical, &$isHandle, $ch): int {{
            if ($received === $ch) {{
                $identical = 1;
            }}
            if ($received instanceof CurlHandle) {{
                $isHandle = 1;
            }}
            return strlen($data);
        }});
        curl_exec($ch);
        echo $identical, $isHandle;
        "#
    ));
    assert_eq!(out, "11");
}

/// `curl_copy_handle()` must re-point the copy's callbacks at the COPY. libcurl's
/// `duphandle` carries the callback function pointers AND their `CURLOPT_*DATA` — which
/// hold the ORIGINAL handle's bridge id — so a copy left as libcurl made it would fire
/// with the original's `$ch` and the original's slots. Measured against PHP 8.4.20: the
/// copy's callback receives the copy.
///
/// The proof is the pair of transfers: the copy's transfer runs the callable but never
/// sees the original handle, and the original's transfer afterwards does.
#[test]
fn curl_copy_handle_callbacks_fire_with_the_copy_not_the_original() {
    if skip_without_curl_native("curl_copy_handle_callbacks_fire_with_the_copy_not_the_original") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        $sawOriginal = 0;
        $bytes = 0;
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $received, string $data) use (&$sawOriginal, &$bytes, $ch): int {{
            if ($received === $ch) {{
                $sawOriginal = $sawOriginal + 1;
            }}
            $bytes = $bytes + strlen($data);
            return strlen($data);
        }});
        $copy = curl_copy_handle($ch);
        curl_exec($copy);
        echo "afterCopy: sawOriginal=", $sawOriginal, " bytes=", $bytes;
        curl_exec($ch);
        echo " afterOriginal: sawOriginal=", $sawOriginal, " bytes=", $bytes;
        "#
    ));
    assert_eq!(
        out,
        "afterCopy: sawOriginal=0 bytes=10 afterOriginal: sawOriginal=1 bytes=20"
    );
}

/// Setting a callback option twice REPLACES the callable: only the second one runs.
#[test]
fn curl_setopt_replaces_a_previously_installed_callback() {
    if skip_without_curl_native("curl_setopt_replaces_a_previously_installed_callback") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $first = '';
        $second = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$first): int {{
            $first .= $data;
            return strlen($data);
        }});
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$second): int {{
            $second .= $data;
            return strlen($data);
        }});
        curl_exec($ch);
        echo "first=", $first, " second=", $second;
        "#
    ));
    assert_eq!(out, "first= second=hello-curl");
}

/// `curl_reset()` drops every installed callback, exactly as php-src's does.
#[test]
fn curl_reset_clears_installed_callbacks() {
    if skip_without_curl_native("curl_reset_clears_installed_callbacks") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $seen = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        curl_reset($ch);
        curl_setopt($ch, CURLOPT_URL, "{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $body = curl_exec($ch);
        echo "body=", $body, " seen=", $seen === '' ? "EMPTY" : $seen;
        "#
    ));
    assert_eq!(out, "body=hello-curl seen=EMPTY");
}

/// Passing `null` to a callback option restores the option's DEFAULT. For
/// `CURLOPT_WRITEFUNCTION` that default is stdout — NOT the `CURLOPT_RETURNTRANSFER`
/// mode that may have been selected earlier, because php-src keeps a single write mode
/// and `null` assigns `PHP_CURL_STDOUT` to it. Measured on PHP 8.4.20.
#[test]
fn curl_setopt_null_restores_the_default_write_path() {
    if skip_without_curl_native("curl_setopt_null_restores_the_default_write_path") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $seen = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        $cleared = curl_setopt($ch, CURLOPT_WRITEFUNCTION, null);
        $r = curl_exec($ch);
        echo "|", $cleared ? "T" : "F";
        echo "|", $r === true ? "TRUE" : "X";
        echo "|", $seen === '' ? "EMPTY" : $seen;
        "#
    ));
    assert_eq!(out, "hello-curl|T|TRUE|EMPTY");
}

/// `CURLOPT_WRITEFUNCTION` and `CURLOPT_RETURNTRANSFER` are two settings of ONE write
/// mode, so the later call wins in both directions. Measured on PHP 8.4.20.
#[test]
fn curl_writefunction_and_returntransfer_are_one_write_mode() {
    if skip_without_curl_native("curl_writefunction_and_returntransfer_are_one_write_mode") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // RETURNTRANSFER set LAST wins: the callback never runs.
        $seen = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $r = curl_exec($ch);
        echo $r === "hello-curl" ? "BODY" : "X", "|", $seen === '' ? "EMPTY" : $seen;

        // WRITEFUNCTION set LAST wins: curl_exec() answers true, not the body.
        $seen2 = '';
        $ch2 = curl_init("{url}");
        curl_setopt($ch2, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch2, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen2): int {{
            $seen2 .= $data;
            return strlen($data);
        }});
        $r2 = curl_exec($ch2);
        echo "|", $r2 === true ? "TRUE" : "X", "|", $seen2;
        "#
    ));
    assert_eq!(out, "BODY|EMPTY|TRUE|hello-curl");
}

/// A named function passed as a string is a valid callback, like any other PHP callable.
#[test]
fn curl_writefunction_accepts_a_named_function_string() {
    if skip_without_curl_native("curl_writefunction_accepts_a_named_function_string") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $collected = '';
        function elephc_test_writer(CurlHandle $ch, string $data): int {{
            global $collected;
            $collected .= $data;
            return strlen($data);
        }}
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, 'elephc_test_writer');
        curl_exec($ch);
        echo $collected;
        "#
    ));
    assert_eq!(out, "hello-curl");
}

/// A value that is not a callable raises PHP's `TypeError` naming the option, rather
/// than being accepted and failing later. Measured wording from PHP 8.4.20.
#[test]
fn curl_setopt_rejects_a_non_callable_callback_value() {
    if skip_without_curl_native("curl_setopt_rejects_a_non_callable_callback_value") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        try {
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, "elephc_no_such_function");
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        try {
            curl_setopt($ch, CURLOPT_HEADERFUNCTION, 42);
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        concat!(
            "curl_setopt(): Argument #3 ($value) must be a valid callback for option ",
            "CURLOPT_WRITEFUNCTION, function \"elephc_no_such_function\" not found or invalid function name\n",
            "curl_setopt(): Argument #3 ($value) must be a valid callback for option ",
            "CURLOPT_HEADERFUNCTION, no array or string given\n"
        )
    );
}

/// An exception thrown inside a callback aborts the transfer and then RESUMES: the
/// adapter's firewall stops the `longjmp` at the libcurl boundary so it cannot unwind
/// through libcurl's own frames, and `curl_exec()` re-raises it on the way out. php-src
/// surfaces the same throw with `curl_errno() === 0`, which is why the aborting
/// `CURLcode` is deliberately not recorded.
#[test]
fn curl_callback_exception_propagates_out_of_curl_exec() {
    if skip_without_curl_native("curl_callback_exception_propagates_out_of_curl_exec") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            throw new RuntimeException("boom");
        }});
        try {{
            curl_exec($ch);
            echo "NOTHROW";
        }} catch (RuntimeException $e) {{
            echo "caught:", $e->getMessage(), "|errno=", curl_errno($ch);
        }}
        echo "|alive";
        "#
    ));
    assert_eq!(out, "caught:boom|errno=0|alive");
}

/// The multi interface is the OTHER place a PHP callback runs — libcurl calls the attached
/// easy handles' callbacks from inside `curl_multi_perform`. A throw there must resume the
/// same way it does out of `curl_exec()`: `__rt_curl_multi_exec` carries the same
/// `__rt_curl_rethrow_pending` hook as `__rt_curl_easy_perform`. Without it the throwable
/// would sit in `_exc_value` and detonate at some unrelated later throw site.
#[test]
fn curl_multi_exec_re_raises_a_callback_exception() {
    if skip_without_curl_native("curl_multi_exec_re_raises_a_callback_exception") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        $mh = curl_multi_init();
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            throw new LogicException("multi-boom");
        }});
        curl_multi_add_handle($mh, $ch);
        $running = 0;
        try {{
            do {{
                $status = curl_multi_exec($mh, $running);
                if ($running > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($running > 0 && $status === CURLM_OK);
            echo "NOTHROW";
        }} catch (LogicException $e) {{
            echo "caught:", $e->getMessage();
        }}
        echo "|alive";
        "#
    ));
    assert_eq!(out, "caught:multi-boom|alive");
}

/// Callback machinery must not leak PER TRANSFER: the argument container, the boxed
/// `$ch`, the boxed arguments, and the invoker's owned return are all released by
/// `__rt_curl_invoke_callback` on both its normal and its exception path. Running the
/// same handles three times instead of once must therefore leak no more than running
/// them once.
///
/// TWO PRE-EXISTING LEAKS ARE DELIBERATELY NOT ASSERTED AWAY HERE, because neither is
/// reachable only through curl and neither is this task's to fix — both reproduce with
/// no curl call in the program (measured with `--gc-stats`):
///   * `__elephc_normalize_callable($closure)` leaks one block per call: the retain it
///     takes on an already-owned descriptor is never released. `Pdo\Sqlite`'s
///     `createFunction`/`createCollation`/`createAggregate`/`setAuthorizer` decompose
///     callables through the identical two-line sequence and leak the same block. That
///     is the "one block per installed callable" this test tolerates.
///   * The descriptor invoker leaks the boxed arguments it materializes into DECLARED
///     parameters. `ob_start(function (string $b, int $p): string {{ … }})` leaks four
///     blocks per flush and `array_map(function (string $s): string {{ … }}, $xs)` leaks
///     per element, both with no curl involved. A callback declaring no parameters is
///     balanced, which is why this fixture uses one: it isolates what the curl adapter
///     itself owns.
#[test]
fn curl_callbacks_do_not_leak_per_transfer() {
    if skip_without_curl_native("curl_callbacks_do_not_leak_per_transfer") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let program = |transfers: usize| {
        let execs = "curl_exec($ch);\n            ".repeat(transfers);
        format!(
            r#"<?php
        function fetch(): void {{
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (): int {{
                return 10;
            }});
            {execs}unset($ch);
        }}
        fetch();
        fetch();
        fetch();
        echo "done";
        "#
        )
    };

    let once = compile_and_run_with_gc_stats(&program(1));
    assert_eq!(once.stdout, "done");
    let (once_allocs, once_frees) = parse_gc_stats(&once.stderr);
    let once_leaked = once_allocs as i64 - once_frees as i64;

    let thrice = compile_and_run_with_gc_stats(&program(3));
    assert_eq!(thrice.stdout, "done");
    let (thrice_allocs, thrice_frees) = parse_gc_stats(&thrice.stderr);
    let thrice_leaked = thrice_allocs as i64 - thrice_frees as i64;

    assert_eq!(
        once_leaked, thrice_leaked,
        "curl callbacks must not leak per transfer (1 transfer leaked {once_leaked}, \
         3 transfers leaked {thrice_leaked})"
    );
    assert_eq!(
        once_leaked, 3,
        "expected exactly one leaked block per installed callable (the pre-existing \
         __elephc_normalize_callable retain), got {once_leaked} for three handles"
    );
}

/// Clearing a read callback (or resetting a handle that had one) must not leave libcurl's
/// DEFAULT `fread` read function pointed at a bogus `FILE *`. `curl_easy_reset` restores
/// `CURLOPT_READDATA` to `stdin`, so a bridge that then wrote the handle id — or `NULL` —
/// into it and restored the default function would segfault on the next upload. The
/// trampoline stays installed and reports end-of-data instead, which is also php-src's
/// behavior (its own `curl_read` with no callable returns 0).
#[test]
fn curl_upload_after_reset_and_after_clearing_the_read_callback_is_safe() {
    if skip_without_curl_native("curl_upload_after_reset_and_after_clearing_the_read_callback_is_safe")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        // (a) a handle whose read callback was installed and then cleared with null
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length): string {{
            return "zzz";
        }});
        curl_setopt($ch, CURLOPT_READFUNCTION, null);
        curl_setopt($ch, CURLOPT_UPLOAD, true);
        // A NONZERO size is what makes libcurl actually CALL the read function; with 0 it
        // never asks, and the hazard this test exists for would go unexercised.
        curl_setopt($ch, CURLOPT_INFILESIZE, 5);
        $r = curl_exec($ch);
        echo "cleared:", $r === false ? "false" : "body", "|", curl_errno($ch);

        // (b) a handle reset after having a read callback, then asked to upload
        $ch2 = curl_init("{url}");
        curl_setopt($ch2, CURLOPT_READFUNCTION, function (CurlHandle $ch, $fd, int $length): string {{
            return "zzz";
        }});
        curl_reset($ch2);
        curl_setopt($ch2, CURLOPT_URL, "{url}");
        curl_setopt($ch2, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($ch2, CURLOPT_UPLOAD, true);
        curl_setopt($ch2, CURLOPT_INFILESIZE, 5);
        $r2 = curl_exec($ch2);
        echo "|reset:", $r2 === false ? "false" : "body", "|", curl_errno($ch2);
        echo "|alive";
        "#
    ));
    // CURLE_READ_ERROR (26): the callback reports end-of-data before the announced
    // CURLOPT_INFILESIZE is satisfied. The load-bearing part is that the program REACHES
    // this line at all — the pre-fix build handed libcurl's default `fread` a null
    // `FILE *` here and died with SIGSEGV.
    assert_eq!(out, "cleared:false|26|reset:false|26|alive");
}

/// REVIEW FIX (Critical 1). `curl_copy_handle()` must not resurrect a write callback that
/// `CURLOPT_RETURNTRANSFER` had deselected on the source. php-src keeps one write mode, so
/// a handle with WRITEFUNCTION-then-RETURNTRANSFER is in RETURN mode with the callable
/// merely rooted; the copy has to start in RETURN mode too. Registering slot 0 anyway
/// would both fire the callback AND (because installing a write callback clears
/// `return_transfer`) make `curl_exec()` on the copy answer an empty capture.
#[test]
fn curl_copy_handle_preserves_returntransfer_over_a_rooted_write_callback() {
    if skip_without_curl_native(
        "curl_copy_handle_preserves_returntransfer_over_a_rooted_write_callback",
    ) {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $seen = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        // RETURNTRANSFER LAST: the write mode is RETURN, the callable is only rooted.
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        $copy = curl_copy_handle($ch);
        $body = curl_exec($copy);
        echo $body === "hello-curl" ? "BODY" : json_encode($body);
        echo "|", $seen === '' ? "SILENT" : $seen;
        // And the source itself still behaves the same way afterwards.
        $body2 = curl_exec($ch);
        echo "|", $body2 === "hello-curl" ? "BODY" : json_encode($body2);
        echo "|", $seen === '' ? "SILENT" : $seen;
        "#
    ));
    assert_eq!(out, "BODY|SILENT|BODY|SILENT");
}

/// REVIEW FIX (Important 2, false-positive direction). `curl_exec()` inside a `finally`
/// that is running with an unmatched throw pending must NOT re-raise that unrelated
/// exception early: the whole `finally` runs, and the original throw resumes afterwards.
/// Gating the re-raise on `_exc_value` instead of on the bridge's own "a callback threw"
/// flag truncated the `finally` here.
#[test]
fn curl_exec_in_a_finally_does_not_re_raise_an_unrelated_pending_throw() {
    if skip_without_curl_native("curl_exec_in_a_finally_does_not_re_raise_an_unrelated_pending_throw")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        function work(string $url): void {{
            try {{
                throw new RuntimeException("outer");
            }} finally {{
                $ch = curl_init($url);
                curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
                $body = curl_exec($ch);
                echo "finally-start|", $body === "hello-curl" ? "BODY" : "X", "|";
                echo "finally-end|";
            }}
        }}
        try {{
            work("{url}");
            echo "NOTHROW";
        }} catch (RuntimeException $e) {{
            echo "caught:", $e->getMessage();
        }}
        "#
    ));
    assert_eq!(out, "finally-start|BODY|finally-end|caught:outer");
}

/// REVIEW FIX (Important 4). After a write callback throws, libcurl still runs its
/// teardown — and with `CURLOPT_VERBOSE` on, teardown `infof()` calls reach the debug
/// callback. Running PHP there would execute user code with the first exception still
/// pending, and a `try/catch` inside that second callback would silently swallow it.
/// Nothing is invoked after the first throw for the rest of the transfer.
#[test]
fn curl_no_callback_runs_after_the_first_throw_in_a_transfer() {
    if skip_without_curl_native("curl_no_callback_runs_after_the_first_throw_in_a_transfer") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $debugRuns = 0;
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_VERBOSE, true);
        curl_setopt($ch, CURLOPT_DEBUGFUNCTION, function (CurlHandle $ch, int $type, string $data) use (&$debugRuns): int {{
            $debugRuns = $debugRuns + 1;
            // A try/catch that actually CATCHES is what swallows the write callback's
            // exception: catching clears the pending-throwable slot the firewall parked.
            try {{
                throw new LogicException("inner");
            }} catch (LogicException $e) {{
                $swallowed = 1;
            }}
            return 0;
        }});
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            throw new RuntimeException("write-boom");
        }});
        $before = 0;
        try {{
            curl_exec($ch);
            echo "NOTHROW";
        }} catch (RuntimeException $e) {{
            echo "caught:", $e->getMessage();
        }}
        // The debug callback ran BEFORE the body arrived (request headers etc.), but must
        // not have run again after the write callback threw.
        echo "|debugRan=", $debugRuns > 0 ? "yes" : "no";
        echo "|alive";
        "#
    ));
    assert_eq!(out, "caught:write-boom|debugRan=yes|alive");
}

/// REVIEW FIX (Important 3). A fresh handle set to `CURLOPT_UPLOAD` with no PHP read
/// callback must not fall through to libcurl's default `fread` on `CURLOPT_READDATA`,
/// whose documented default is the PROCESS'S OWN STDIN. The read trampoline is now
/// installed at `curl_init()` (as php-src installs its `curl_read`), not lazily on the
/// first callback `curl_setopt()`.
///
/// WHAT THIS PINS is the asymmetry the fix removes: a FRESH handle and the SAME handle
/// after `curl_reset()` must behave identically, because `curl_reset()` used to be the
/// only path that installed the trampoline. Both now report end-of-data.
///
/// It does NOT execute the stdin leak itself. The codegen harness runs compiled binaries
/// with stdin INHERITED and has no curl-capable stdin-piping variant, so a program that
/// read the ambient stdin would either see whatever the test runner was given or block on
/// a terminal — flaky either way. The leak is established by libcurl's documented default
/// rather than by execution; see the fix report.
#[test]
fn curl_fresh_handle_upload_matches_post_reset_upload() {
    if skip_without_curl_native("curl_fresh_handle_upload_matches_post_reset_upload") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/echo");
    let out = compile_and_run(&format!(
        r#"<?php
        // (a) never touched by any callback option
        $fresh = curl_init("{url}");
        curl_setopt($fresh, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($fresh, CURLOPT_UPLOAD, true);
        curl_setopt($fresh, CURLOPT_INFILESIZE, 5);
        $a = curl_exec($fresh);
        echo "fresh:", $a === false ? "false" : "body", "|", curl_errno($fresh);

        // (b) the same handle after a reset — must behave identically
        curl_reset($fresh);
        curl_setopt($fresh, CURLOPT_URL, "{url}");
        curl_setopt($fresh, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($fresh, CURLOPT_UPLOAD, true);
        curl_setopt($fresh, CURLOPT_INFILESIZE, 5);
        $b = curl_exec($fresh);
        echo "|reset:", $b === false ? "false" : "body", "|", curl_errno($fresh);

        // (c) and a zero-length upload succeeds with an empty body on a fresh handle
        $zero = curl_init("{url}");
        curl_setopt($zero, CURLOPT_RETURNTRANSFER, true);
        curl_setopt($zero, CURLOPT_UPLOAD, true);
        curl_setopt($zero, CURLOPT_INFILESIZE, 0);
        $c = curl_exec($zero);
        echo "|zero:", strpos($c, "body=") !== false ? "empty" : "X", "|", curl_errno($zero);
        "#
    ));
    // 26 = CURLE_READ_ERROR: end-of-data before the announced CURLOPT_INFILESIZE.
    assert_eq!(out, "fresh:false|26|reset:false|26|zero:empty|0");
}

/// REVIEW FIX (Important 7). A callback throw must not leave a PREVIOUS transfer's
/// `curl_errno()`/`curl_error()` visible on a reused handle: php-src reports `0` for a
/// transfer that ended in a PHP exception, and `0` is what code inspecting the handle
/// inside `catch` has to see.
#[test]
fn curl_callback_throw_clears_a_previous_transfers_error_state() {
    if skip_without_curl_native("curl_callback_throw_clears_a_previous_transfers_error_state") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        // First transfer fails for a real libcurl reason.
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
        curl_exec($ch);
        echo "first=", curl_errno($ch) !== 0 ? "err" : "ok";
        echo "|msg=", strlen(curl_error($ch)) > 0 ? "set" : "empty";

        // Same handle, now a callback throws: the old error must not survive.
        curl_setopt($ch, CURLOPT_URL, "{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            throw new RuntimeException("boom");
        }});
        try {{
            curl_exec($ch);
            echo "|NOTHROW";
        }} catch (RuntimeException $e) {{
            echo "|caught|errno=", curl_errno($ch), "|msg=", curl_error($ch) === "" ? "empty" : curl_error($ch);
        }}
        "#
    ));
    assert_eq!(out, "first=err|msg=set|caught|errno=0|msg=empty");
}

/// REVIEW FIX (Important 6) — PINS A KNOWN LIMITATION, NOT A GUARANTEE.
///
/// A callback that captures its own `CurlHandle` closes a reference cycle
/// (`CurlHandle -> __elephc_callbacks -> Closure -> $ch`). elephc is refcount-only with no
/// cycle collector, so the handle is never freed and its libcurl session lives until the
/// process exits. php-src has the identical cycle and survives it only because Zend has a
/// cycle collector.
///
/// The non-capturing shape is balanced apart from the one pre-existing
/// `__elephc_normalize_callable` block per installed callable, so the DIFFERENCE between
/// the two programs is the cycle's cost. UPDATE THIS TEST WHEN A CYCLE COLLECTOR LANDS:
/// the capturing program should then leak no more than the non-capturing one.
#[test]
fn curl_callback_capturing_its_own_handle_leaks_the_session() {
    if skip_without_curl_native("curl_callback_capturing_its_own_handle_leaks_the_session") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    // IDENTICAL closure shapes apart from the captured handle: no declared parameters (a
    // declared parameter hits the separate, pre-existing invoker argument leak and would
    // swamp the signal), same by-ref counter, same body.
    let program = |capture: bool| {
        let use_clause = if capture {
            "use (&$total, $ch)"
        } else {
            "use (&$total)"
        };
        format!(
            r#"<?php
        function fetch(): void {{
            $total = 0;
            $ch = curl_init("{url}");
            curl_setopt($ch, CURLOPT_WRITEFUNCTION, function () {use_clause}: int {{
                $total = $total + 1;
                return 10;
            }});
            curl_exec($ch);
            unset($ch);
        }}
        fetch();
        fetch();
        fetch();
        echo "done";
        "#
        )
    };

    let plain = compile_and_run_with_gc_stats(&program(false));
    assert_eq!(plain.stdout, "done");
    let (plain_allocs, plain_frees) = parse_gc_stats(&plain.stderr);
    let plain_leaked = plain_allocs as i64 - plain_frees as i64;

    let capturing = compile_and_run_with_gc_stats(&program(true));
    assert_eq!(capturing.stdout, "done");
    let (cap_allocs, cap_frees) = parse_gc_stats(&capturing.stderr);
    let cap_leaked = cap_allocs as i64 - cap_frees as i64;

    // No exact number is asserted for the non-capturing shape: it carries pre-existing
    // per-callable and by-ref-capture leaks that are not this task's and would make the
    // pin a tripwire for unrelated work. The COMPARISON is the limitation being recorded.
    assert!(
        cap_leaked > plain_leaked,
        "capturing $ch is expected to leak the whole libcurl session today (elephc is \
         refcount-only, with no cycle collector): capturing leaked {cap_leaked}, \
         non-capturing leaked {plain_leaked}. If this now fails because the two are \
         equal, a cycle collector landed — delete this test and the limitation note at \
         the callback-rooting site in src/curl_prelude.rs."
    );
}

/// REVIEW FIX ROUND 2. The "no callback runs after a throw" gate must be scoped to ONE
/// TRANSFER, not to the handle forever. Keying it on the per-handle flag left it sticky on
/// the multi path — which never cleared it — so a handle whose callback threw during
/// `curl_multi_exec()` had its callbacks silently disabled for the rest of the program:
/// every later transfer aborted with `CURLE_WRITE_ERROR` and the callable was never
/// invoked. php-src reuses such a handle normally.
#[test]
fn curl_handle_is_reusable_after_a_callback_threw_during_multi_exec() {
    if skip_without_curl_native("curl_handle_is_reusable_after_a_callback_threw_during_multi_exec") {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/a");
    let out = compile_and_run(&format!(
        r#"<?php
        function drive(CurlMultiHandle $mh): int {{
            $running = 0;
            do {{
                $status = curl_multi_exec($mh, $running);
                if ($running > 0) {{
                    curl_multi_select($mh, 1.0);
                }}
            }} while ($running > 0 && $status === CURLM_OK);
            return $status;
        }}

        $mh = curl_multi_init();
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data): int {{
            throw new LogicException("multi-boom");
        }});
        curl_multi_add_handle($mh, $ch);
        try {{
            drive($mh);
            echo "NOTHROW";
        }} catch (LogicException $e) {{
            echo "caught:", $e->getMessage();
        }}
        curl_multi_remove_handle($mh, $ch);

        // SAME HANDLE, no curl_reset(): a fresh callable must fire again and the transfer
        // must complete. This is the regression: the callback used to be dead forever.
        $seen = '';
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        curl_multi_add_handle($mh, $ch);
        $status = drive($mh);
        echo "|reuse:", $seen === '' ? "DEAD" : $seen;
        echo "|", $status === CURLM_OK ? "ok" : "err";

        // …and on the EASY interface too.
        $seen2 = '';
        curl_multi_remove_handle($mh, $ch);
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen2): int {{
            $seen2 .= $data;
            return strlen($data);
        }});
        curl_exec($ch);
        echo "|easy:", $seen2 === '' ? "DEAD" : $seen2, "|", curl_errno($ch);
        "#
    ));
    assert_eq!(
        out,
        "caught:multi-boom|reuse:body-a|ok|easy:body-a|0"
    );
}

/// REVIEW FIX ROUND 2 (STDOUT arm). `CURLOPT_RETURNTRANSFER => false` after a
/// `CURLOPT_WRITEFUNCTION` selects php-src's third write mode, PHP_CURL_STDOUT, leaving the
/// callable rooted but inactive. The copy must land in STDOUT mode too — deciding on
/// `__elephc_return_transfer` alone could not tell RETURN from STDOUT, so the copy
/// re-selected PHP_CURL_USER and the callback fired where php prints.
#[test]
fn curl_copy_handle_preserves_stdout_mode_over_a_rooted_write_callback() {
    if skip_without_curl_native("curl_copy_handle_preserves_stdout_mode_over_a_rooted_write_callback")
    {
        return;
    }
    let server = LocalHttpServer::spawn_hello();
    let url = server.url("/hello");
    let out = compile_and_run(&format!(
        r#"<?php
        $seen = '';
        $ch = curl_init("{url}");
        curl_setopt($ch, CURLOPT_WRITEFUNCTION, function (CurlHandle $ch, string $data) use (&$seen): int {{
            $seen .= $data;
            return strlen($data);
        }});
        // Selects PHP_CURL_STDOUT: the callable stays rooted but stops being the sink.
        curl_setopt($ch, CURLOPT_RETURNTRANSFER, false);
        $copy = curl_copy_handle($ch);
        $r = curl_exec($copy);
        echo "|", $r === true ? "TRUE" : "X";
        echo "|", $seen === '' ? "SILENT" : $seen;
        "#
    ));
    // The body is printed by the copy (stdout mode) before the echoes.
    assert_eq!(out, "hello-curl|TRUE|SILENT");
}

/// The callback options this wave deliberately did NOT implement stay honestly rejected:
/// `curl_setopt()` answers `false` and emits PHP's unsupported-option warning rather than
/// an inert `true` (locked decision 7).
#[test]
fn curl_remaining_callback_options_stay_rejected() {
    if skip_without_curl_native("curl_remaining_callback_options_stay_rejected") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        $cb = function (): int { return 0; };
        echo curl_setopt($ch, CURLOPT_FNMATCH_FUNCTION, $cb) ? "T" : "F";
        echo curl_setopt($ch, CURLOPT_PREREQFUNCTION, $cb) ? "T" : "F";
        echo curl_setopt($ch, CURLOPT_SSH_HOSTKEYFUNCTION, $cb) ? "T" : "F";
        "#,
    );
    assert_eq!(out, "FFF");
}
