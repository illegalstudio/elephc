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
