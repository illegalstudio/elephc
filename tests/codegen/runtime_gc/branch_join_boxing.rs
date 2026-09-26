//! Purpose:
//! Heap-debug regression tests for issue #771: locals joined across divergent `if` arms are boxed
//! at the merge, and every value they held along the way must still be released.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` in a loop and asserts `leak summary: clean`, so a
//!   per-call leak shows up as a non-clean summary instead of hiding in the noise of one call.
//! - Expected output was produced by PHP 8.5.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// The #771 repro in a loop: the edge-joined capture releases every string it held.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_string_capture() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = "k" . $argc;
$f = function (int $n) use ($m) { $m = null; if ($n > 1) { $m = "s" . $n; } return $m; };
$total = 0;
for ($i = 0; $i < 200; $i++) { $total += strlen((string) $f($i % 3)); }
echo $total, "\n";
"#,
    );
    assert_clean(out, "132\n");
}

/// A `null`-or-int local is boxed on each merge edge; the boxes the backend made for the earlier
/// scalar stores must be released by the deferred slot release, not leaked once per call.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_scalar_word_edge_boxing() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$d = function (int $n) { $v = null; if ($n % 2 === 0) { $v = $n; $v = $v + 1; } return $v; };
$w = function (int $n) { $v = 0; $v = null; if ($n % 3 === 0) { $v = true; } return $v; };
$total = 0;
for ($i = 0; $i < 200; $i++) { $total += (int) $d($i) + ($w($i) === true ? 1 : 0); }
echo $total, "\n";
"#,
    );
    assert_clean(out, "10067\n");
}

/// Float/string, object and array arms joined inside a loop leave the heap clean.
#[test]
fn test_closure_local_retype_branch_join_heap_clean_mixed_arms() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$g = function (int $n) { $v = 1; if ($n % 3 === 1) { $v = 2.5; } elseif ($n % 3 === 2) { $v = "x" . $n; } return $v; };
$h = function (int $n) { $v = null; if ($n % 2 === 0) { $v = [$n, "e" . $n]; } return $v; };
$o = function (int $n) { $v = null; if ($n % 2 === 0) { $v = new ArrayObject([$n]); } return $v; };
$total = 0;
for ($i = 0; $i < 200; $i++) {
    $total += strlen((string) $g($i)) + count($h($i) ?? []) + count($o($i) ?? []);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "796\n");
}
