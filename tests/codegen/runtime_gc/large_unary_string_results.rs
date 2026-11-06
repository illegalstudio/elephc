//! Purpose:
//! Heap-debug regressions for large strings returned by unary string runtime helpers.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` in the codegen integration test suite.
//!
//! Key details:
//! - Results larger than the concat scratch buffer are owned heap blocks and must be released
//!   when a consumer such as `strlen()` discards them.
//! - Repeated calls expose ownership errors that a single temporary allocation can hide.

use crate::support::compile_and_run_with_heap_debug;

/// Large unary-string results remain balanced after repeated consuming calls.
#[test]
fn test_large_addslashes_results_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$big = str_repeat("a'b", 30000);
for ($i = 0; $i < 5; $i++) { $length = strlen(addslashes($big)); }
echo $length;
"#,
    );
    assert_eq!(out.stdout, "120000", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "large unary-string results leaked: {}",
        out.stderr
    );
}

/// Large results of builtins that never alias an argument are released after being copied.
///
/// Such a result lives in concat scratch while it fits and in an owned heap block past
/// 64 KiB. Keeping it in a local copies it, so the original block still needs its release.
#[test]
fn test_large_independent_string_results_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function kept(): int { $big = str_repeat("a'b", 30000); return strlen($big); }
function replaced(): int { return strlen(str_replace("a", "bb", str_repeat("a'b", 30000))); }
function escaped(): int { return strlen(htmlspecialchars(str_repeat("<a>", 30000))); }
function split_up(): int { $s = chunk_split(str_repeat("xy", 40000), 2, "|"); return strlen($s); }
function run(): void {
    $total = 0;
    for ($i = 0; $i < 4; $i++) { $total += kept() + replaced() + escaped() + split_up(); }
    echo $total, "\n";
}
run();
"#,
    );
    assert_eq!(out.stdout, "2400000\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "large independent string results leaked: {}",
        out.stderr
    );
}
