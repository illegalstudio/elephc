//! Purpose:
//! End-to-end fixtures for the curl easy handle's object surface and for
//! `curl_version()`: `curl_init()` returns a real `CurlHandle` object, serializing it
//! throws, `function_exists()` and `extension_loaded()` agree that curl is present, and
//! `curl_version()` reports the libcurl this binary actually linked.
//!
//! Called from:
//! - `cargo test --test codegen_tests curl` through Rust's test harness.
//!
//! Key details:
//! - No fixture here touches the network. `curl_version()` reads
//!   `curl_version_info(CURLVERSION_NOW)` out of the linked library; everything else is
//!   object identity and declaration checks.
//! - The asserted version is elephc's PINNED libcurl (8.21.0, locked decision 1), never
//!   the developer's system curl — that is the whole point of the managed native
//!   package, so this assertion is the pin's end-to-end proof.
//! - `CurlHandle` being `final` and not user-constructible is a COMPILE-TIME diagnostic,
//!   so those two checks live in `tests/error_tests/curl.rs` instead: they need only the
//!   injected prelude, never a link, and therefore run everywhere.

use crate::support::*;

/// `curl_init()` mints a real PHP object of the `CurlHandle` class, not a resource and
/// not an int: `get_class()` names it and `instanceof` agrees, exactly as PHP 8 does
/// since the resource-to-object migration.
#[test]
fn curl_init_returns_curlhandle() {
    if skip_without_curl_native("curl_init_returns_curlhandle") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo get_class($ch), "\n";
        echo ($ch instanceof CurlHandle) ? "yes\n" : "no\n";
        "#,
    );
    assert_eq!(out, "CurlHandle\nyes\n");
}

/// `curl_version()` reports the pinned libcurl the binary actually linked — 8.21.0 from
/// the managed native package, with HTTP in its protocol list — decoded from the
/// bridge's JSON blob into PHP's associative array shape.
#[test]
fn curl_version_reports_pinned_libcurl() {
    if skip_without_curl_native("curl_version_reports_pinned_libcurl") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo $v['version'], "\n";
        $found = false;
        foreach ($v['protocols'] as $protocol) {
            if ($protocol === 'http') {
                $found = true;
            }
        }
        echo $found ? "http\n" : "no-http\n";
        "#,
    );
    assert!(out.starts_with("8.21.0\n"), "{out}");
    assert!(out.contains("http\n"), "{out}");
}

/// `curl_version()` carries the rest of PHP's documented key set, and the numeric keys
/// really are numbers rather than the JSON blob's text — proof the array came through
/// `json_decode()`'s typed decoding and not a string split.
#[test]
fn curl_version_exposes_php_key_shape() {
    if skip_without_curl_native("curl_version_exposes_php_key_shape") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $v = curl_version();
        echo is_array($v) ? "array\n" : "not-array\n";
        echo is_int($v['version_number']) ? "int\n" : "not-int\n";
        echo is_int($v['features']) ? "int\n" : "not-int\n";
        echo is_string($v['host']) ? "string\n" : "not-string\n";
        echo is_array($v['protocols']) ? "protocols\n" : "no-protocols\n";
        echo array_key_exists('ssl_version', $v) ? "ssl\n" : "no-ssl\n";
        "#,
    );
    assert_eq!(out, "array\nint\nint\nstring\nprotocols\nssl\n");
}

/// The PHP names come from the injected prelude, so `function_exists()` sees them
/// exactly like php-src's own `ext/curl` registrations.
#[test]
fn curl_functions_are_declared() {
    if skip_without_curl_native("curl_functions_are_declared") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo function_exists('curl_init') ? "init\n" : "no-init\n";
        echo function_exists('curl_exec') ? "exec\n" : "no-exec\n";
        echo function_exists('curl_setopt') ? "setopt\n" : "no-setopt\n";
        echo function_exists('curl_version') ? "version\n" : "no-version\n";
        echo class_exists('CurlHandle') ? "class\n" : "no-class\n";
        "#,
    );
    assert_eq!(out, "init\nexec\nsetopt\nversion\nclass\n");
}

/// A curl-using program reports `curl` as a loaded extension, because the bridge that
/// backs it is linked. The negative half of this contract (a curl-free program still
/// reports curl unloaded) lives in `tests/extension_loaded_tests.rs`.
#[test]
fn extension_loaded_curl_is_true_for_a_curl_program() {
    if skip_without_curl_native("extension_loaded_curl_is_true_for_a_curl_program") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo extension_loaded('curl') ? "loaded\n" : "unloaded\n";
        "#,
    );
    assert_eq!(out, "loaded\n");
}

/// Serializing a `CurlHandle` throws, exactly as php-src does
/// (`Exception: Serialization of 'CurlHandle' is not allowed`) — elephc could not
/// reproduce a live libcurl handle from a serialized blob, so failing loudly is the
/// honest answer rather than emitting a plausible-looking dead object.
#[test]
fn curl_handle_serialization_throws() {
    if skip_without_curl_native("curl_handle_serialization_throws") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        try {
            $s = serialize($ch);
            echo "serialized\n";
        } catch (\Exception $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(out, "Serialization of 'CurlHandle' is not allowed\n");
}

/// PHP function names are case-insensitive and namespace-qualified spellings resolve to
/// the global function, so `CURL_INIT()` and `\curl_init()` reach the same prelude
/// wrapper the lowercase spelling does.
#[test]
fn curl_names_are_case_insensitive_and_namespace_qualified() {
    if skip_without_curl_native("curl_names_are_case_insensitive_and_namespace_qualified") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $a = CURL_INIT();
        $b = \curl_init();
        echo get_class($a), "\n";
        echo get_class($b), "\n";
        "#,
    );
    assert_eq!(out, "CurlHandle\nCurlHandle\n");
}

/// `curl_close()` is a no-op in PHP 8: the handle stays usable afterwards and the object
/// still frees its libcurl handle exactly once when it goes out of scope.
#[test]
fn curl_close_is_a_no_op() {
    if skip_without_curl_native("curl_close_is_a_no_op") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        curl_close($ch);
        echo ($ch instanceof CurlHandle) ? "alive\n" : "dead\n";
        echo curl_errno($ch), "\n";
        "#,
    );
    assert_eq!(out, "alive\n0\n");
}

/// A handle that never performed a transfer reports `CURLE_OK` and an empty error
/// string, matching PHP's own initial state for a fresh handle.
#[test]
fn fresh_handle_reports_no_error() {
    if skip_without_curl_native("fresh_handle_reports_no_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_errno($ch), "\n";
        echo strlen(curl_error($ch)), "\n";
        "#,
    );
    assert_eq!(out, "0\n0\n");
}

/// `curl_setopt()` forwards the options this build really supports and reports libcurl's
/// own acceptance, and rejects a value type it cannot carry with PHP 8's `TypeError`
/// (php-src raises a TypeError, not a ValueError, for a wrong-typed `$value`).
#[test]
fn curl_setopt_forwards_and_rejects_honestly() {
    if skip_without_curl_native("curl_setopt_forwards_and_rejects_honestly") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        echo curl_setopt($ch, 10002, "http://127.0.0.1:1/") ? "url\n" : "no-url\n";
        echo curl_setopt($ch, 19913, true) ? "capture\n" : "no-capture\n";
        echo curl_setopt($ch, 52, 1) ? "follow\n" : "no-follow\n";
        try {
            curl_setopt($ch, 10002, [1, 2]);
            echo "accepted\n";
        } catch (\TypeError $e) {
            echo $e->getMessage(), "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "url\ncapture\nfollow\n\
         curl_setopt(): Argument #3 ($value) must be of type string|int|float|bool, array given\n"
    );
}

/// An option php-src RECOGNIZES but this build cannot carry is rejected BEFORE it
/// reaches libcurl: `curl_setopt()` answers `false` and emits PHP's warning (locked
/// decision 7), rather than the inert `true` libcurl would have produced after being
/// handed a PHP value as a function pointer / `struct curl_blob *` / PHP stream.
///
/// This is a memory-safety regression test, not a politeness one. Every option here used
/// to be forwarded verbatim: 20011 (`CURLOPT_WRITEFUNCTION`) overwrote the bridge's own
/// write callback with the address `1`, and 40291 (`CURLOPT_SSLCERT_BLOB`) mis-read a PHP
/// string as a `struct curl_blob *`. 10001 (`CURLOPT_FILE`) is the PHP-stream option a
/// later task owns. 10100 (`CURLOPT_SHARE`) used to be in this list too — Task 10 makes it
/// work; `tests/codegen/curl/share.rs` covers it now, including its own honest rejection
/// (a non-`CurlShareHandle` value is a `TypeError`, not a silent `false`). The transfer at
/// the end is what proves nothing was corrupted: the handle still performs and still
/// reports its own error, so none of the rejected options reached libcurl's state.
#[test]
fn unsupported_options_are_rejected_before_libcurl() {
    if skip_without_curl_native("unsupported_options_are_rejected_before_libcurl") {
        return;
    }
    let output = compile_and_run_capture(
        r#"<?php
        $ch = curl_init();
        echo curl_setopt($ch, 20011, 1) ? "accepted\n" : "rejected\n";
        echo curl_setopt($ch, 40291, "blob") ? "accepted\n" : "rejected\n";
        echo curl_setopt($ch, 10001, 1) ? "accepted\n" : "rejected\n";
        curl_setopt($ch, 10002, "file:///nonexistent-elephc-curl-probe");
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo ($body === false) ? "exec-false\n" : "exec-ok\n";
        echo (curl_errno($ch) !== 0) ? "errno\n" : "no-errno\n";
        "#,
    );
    assert_eq!(
        output.stdout,
        "rejected\nrejected\nrejected\nexec-false\nerrno\n"
    );
    for option in ["20011", "40291", "10001"] {
        assert!(
            output.stderr.contains(&format!(
                "Warning: curl_setopt(): Option {option} is not supported by this build"
            )),
            "each rejected option must warn; stderr was: {}",
            output.stderr
        );
    }
}

/// An option number that is not a cURL option AT ALL raises php-src's own
/// `ValueError: curl_setopt(): Argument #2 ($option) is not a valid cURL option`, which
/// is a DIFFERENT answer from the `false` + warning an unsupported-but-real option gets.
///
/// php-src's `_php_curl_setopt` ends its switch with exactly this
/// `zend_argument_value_error(2, ...)`, so a program that mistypes an option number finds
/// out immediately instead of watching a `false` it may not even check. The last case is
/// the one a 32-bit option parameter would have got wrong: `4294967298` truncates onto
/// option `2`, which IS a real option (`CURLINFO_HEADER_OUT`), and must still throw.
#[test]
fn invalid_option_numbers_raise_a_value_error() {
    if skip_without_curl_native("invalid_option_numbers_raise_a_value_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init();
        foreach ([9998, 30005, 40077, 4294967298] as $option) {
            try {
                curl_setopt($ch, $option, 1);
                echo "accepted\n";
            } catch (\ValueError $e) {
                echo $e->getMessage(), "\n";
            }
        }
        echo curl_setopt($ch, 52, 1) ? "follow\n" : "no-follow\n";
        "#,
    );
    let message = "curl_setopt(): Argument #2 ($option) is not a valid cURL option";
    assert_eq!(
        out,
        format!("{message}\n{message}\n{message}\n{message}\nfollow\n")
    );
}

/// A transfer against a closed loopback port fails honestly: `curl_exec()` returns
/// `false`, `curl_errno()` reports a real non-zero `CURLcode`, and `curl_error()`
/// carries libcurl's own message. No network beyond localhost is involved.
#[test]
fn failed_transfer_reports_curl_error() {
    if skip_without_curl_native("failed_transfer_reports_curl_error") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo ($body === false) ? "false\n" : "not-false\n";
        echo (curl_errno($ch) !== 0) ? "errno\n" : "no-errno\n";
        echo (strlen(curl_error($ch)) > 0) ? "message\n" : "no-message\n";
        "#,
    );
    assert_eq!(out, "false\nerrno\nmessage\n");
}

/// A complete transfer round-trips through the whole chain — `curl_setopt` →
/// `curl_exec` → captured body — over the `file://` protocol, so the assertion covers
/// perform, the RETURNTRANSFER capture, and the borrowed-buffer copy without touching a
/// socket. `file` is in the first landing's protocol matrix (locked decision 6).
#[test]
fn file_protocol_transfer_returns_the_body() {
    if skip_without_curl_native("file_protocol_transfer_returns_the_body") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "hello from file
");
        $ch = curl_init("file://" . $path);
        curl_setopt($ch, 19913, true);
        $body = curl_exec($ch);
        echo is_string($body) ? "string
" : "not-string
";
        echo $body;
        echo curl_errno($ch), "
";
        unlink($path);
        "#,
    );
    assert_eq!(out, "string
hello from file
0
");
}

/// Without `CURLOPT_RETURNTRANSFER`, `curl_exec()` writes the body straight to stdout and
/// returns `true` — PHP CLI's default shape, and the other half of `curl_exec()`'s
/// three-way return contract.
#[test]
fn transfer_without_returntransfer_writes_to_stdout() {
    if skip_without_curl_native("transfer_without_returntransfer_writes_to_stdout") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "streamed body
");
        $ch = curl_init("file://" . $path);
        $result = curl_exec($ch);
        echo ($result === true) ? "true
" : "not-true
";
        unlink($path);
        "#,
    );
    assert_eq!(out, "streamed body
true
");
}

/// `curl_setopt_array()` applies every option and stops at the first rejection, matching
/// PHP's documented short-circuit.
#[test]
fn curl_setopt_array_applies_every_option() {
    if skip_without_curl_native("curl_setopt_array_applies_every_option") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
        $path = tempnam(sys_get_temp_dir(), "elephc_curl_");
        file_put_contents($path, "batched
");
        $ch = curl_init();
        $ok = curl_setopt_array($ch, [10002 => "file://" . $path, 19913 => true]);
        echo $ok ? "ok
" : "failed
";
        echo curl_exec($ch);
        unlink($path);
        "#,
    );
    assert_eq!(out, "ok
batched
");
}

/// The `CurlHandle` object owns its libcurl handle and releases it exactly once: the
/// heap is balanced after a scope that creates and drops several handles, so the
/// Mixed-cell destructor path really does reach `__rt_curl_easy_free`.
#[test]
fn curl_handles_free_their_native_handle() {
    if skip_without_curl_native("curl_handles_free_their_native_handle") {
        return;
    }
    let output = compile_and_run_with_gc_stats(
        r#"<?php
        function make(): void {
            $ch = curl_init("http://127.0.0.1:1/");
            unset($ch);
        }
        make();
        make();
        make();
        echo "done\n";
        "#,
    );
    assert_eq!(output.stdout, "done\n");
    let (allocs, frees) = parse_gc_stats(&output.stderr);
    assert_eq!(allocs, frees, "curl handle scope must be heap-balanced");

    // The balanced count above proves nothing leaked; heap debug proves nothing was freed
    // TWICE or used after release, which is the failure mode a second free path (an
    // over-eager `curl_close`, or a `__destruct` beside the Mixed cell) would introduce.
    let debug = compile_and_run_with_heap_debug(
        r#"<?php
        $ch = curl_init("http://127.0.0.1:1/");
        curl_setopt($ch, 19913, true);
        echo curl_errno($ch), "\n";
        unset($ch);
        echo "done\n";
        "#,
    );
    assert_eq!(debug.stdout, "0\ndone\n", "stderr: {}", debug.stderr);
}
