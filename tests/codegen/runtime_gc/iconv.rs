//! Purpose:
//! Heap-debug regression coverage for the owned results of PHP's iconv builtins.
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_gc::iconv` through the runtime-GC suite.
//!
//! Key details:
//! - Each result kind is exercised: a boxed string, a boxed integer, a boxed associative
//!   array of strings, and one whose values are nested string lists.
//! - The bridge allocates its own buffers and the runtime copies out of them, so a leak
//!   here means either the copy or the bridge release stopped happening.

use crate::support::*;

/// Verifies every iconv result kind releases both its box and its payload.
#[test]
fn test_iconv_owned_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
for ($i = 0; $i < 25; $i++) {
    $converted = iconv("UTF-8", "ISO-8859-1", "café");
    $length = iconv_strlen("héllo");
    $slice = iconv_substr("héllo", 1, 3);
    $encoded = iconv_mime_encode("Subject", "Prüfung");
    $decoded = iconv_mime_decode("Subject: =?ISO-8859-1?Q?Pr=FCfung?=");
    unset($converted, $length, $slice, $encoded, $decoded);
}
echo "clean";
"##,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected iconv() results to leave a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the mbstring case pair releases its string results and thrown `ValueError`s.
///
/// The string entry point persists the bridge's bytes into a runtime string and the
/// refused-encoding path hands a persisted message to the exception, so a leak here means
/// the copy, the bridge release, or the exception's ownership of its message broke.
///
/// The conversions and the throw live in separate functions; the same work inlined in one
/// top-level loop body is `test_string_locals_unset_after_a_caught_value_error_are_heap_clean`.
#[test]
fn test_mb_case_results_and_value_errors_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
function convert_all(int $i, ?string $encoding): int {
    $upper = mb_strtoupper("straße éà " . $i);
    $lower = mb_strtolower("ΣΑΣ " . $i, $encoding);
    $latin = mb_strtoupper("\xe9t\xe9", "ISO-8859-1");
    return strlen($upper) + strlen($lower) + strlen($latin);
}
function refuse(int $i): string {
    try {
        mb_strtoupper("x", "nope" . $i);
    } catch (ValueError $e) {
        return $e->getMessage();
    }
    return "";
}
$encoding = $argc > 5 ? "UTF-8" : null;
$total = 0;
for ($i = 0; $i < 25; $i++) {
    $total += convert_all($i, $encoding);
    $message = refuse($i);
}
echo $total === 655 && strlen($message) === 81 ? "clean" : "wrong $total " . strlen($message);
"##,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected mb_strtoupper()/mb_strtolower() to leave a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies string locals `unset()` after a caught `ValueError` in the same top-level loop body
/// are released exactly once.
///
/// This shape (a `strtoupper()` / `str_repeat()` / `mb_strtoupper()` result, then a `try`/`catch`
/// that catches a builtin's `ValueError` or a user `throw`, then `unset()`) was once documented
/// as a double free; it pins the heap clean so a regression of that kind fails here.
/// php 8.5.10 prints `81 79`.
#[test]
fn test_string_locals_unset_after_a_caught_value_error_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
$encoding = $argc > 5 ? "UTF-8" : null;
for ($i = 0; $i < 25; $i++) {
    $upper = mb_strtoupper("straße éà " . $i);
    $lower = mb_strtolower("ΣΑΣ " . $i, $encoding);
    try {
        mb_strtoupper("x", "nope" . $i);
    } catch (ValueError $e) {
        $message = $e->getMessage();
    }
    unset($upper, $lower);
}
for ($i = 0; $i < 25; $i++) {
    $s = str_repeat("ab", $i + 1);
    try {
        $t = mb_strtoupper($s, "nope");
    } catch (ValueError $e) {
        $m = $e->getMessage();
    }
    unset($s);
}
for ($i = 0; $i < 25; $i++) {
    $s = strtoupper("x" . $i);
    try {
        throw new ValueError("v" . $i);
    } catch (ValueError $e) {
    }
    unset($s);
}
echo strlen($message), " ", strlen($m), "\n";
"##,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "81 79\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected the unset string locals to be released exactly once, got: {}",
        out.stderr
    );
}

/// Verifies the array-returning builtins release their hash, keys, and nested lists.
#[test]
fn test_iconv_array_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
for ($i = 0; $i < 25; $i++) {
    $encodings = iconv_get_encoding();
    $headers = iconv_mime_decode_headers("A: 1\r\nTo: a@b.c\r\nTo: d@e.f");
    unset($encodings, $headers);
}
echo "clean";
"##,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected iconv array results to leave a clean heap, got: {}",
        out.stderr
    );
}
