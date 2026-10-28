//! Purpose:
//! Heap-debug regression coverage for owned `parse_url()` array and component results.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime runtime_gc::parse_url` through the runtime-GC suite.
//!
//! Key details:
//! - Each iteration releases both a Mixed-valued hash and a selected owned string,
//!   exercising copied component payloads and the Mixed cells stored inside the hash.

use crate::support::*;

/// Verifies repeated full-array and selected-string results release every owned allocation.
#[test]
fn test_parse_url_owned_results_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r##"<?php
for ($i = 0; $i < 25; $i++) {
    $parts = parse_url("https://user:pass@example.com:8080/path?q=1#frag");
    $host = parse_url("https://user:pass@example.com:8080/path?q=1#frag", PHP_URL_HOST);
    unset($parts, $host);
}
echo "clean";
"##,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected parse_url() results to leave a clean heap, got: {}",
        out.stderr
    );
}
