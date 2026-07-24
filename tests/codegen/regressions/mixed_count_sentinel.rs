//! Purpose:
//! Regression tests for issue #602: `count()` on a boxed `Mixed` receiver whose payload is
//! the null-container sentinel (produced by a missed array read forwarded through a ternary
//! merge) must raise PHP's `count()` `TypeError` instead of dereferencing the sentinel and
//! segfaulting.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `$argc` keeps the ternary runtime-unknown so the merge (and the missed-read arm) survive
//!   AST folding; the tests run with no arguments, so the missed-read arm executes.
//! - The sentinel, a null pointer, and a boxed null cell (tag 8, e.g. `json_decode("null")`)
//!   must all behave identically — PHP's `count(): Argument #1 ($value) must be of type
//!   Countable|array, null given` — while a real container (including an empty one) still
//!   counts correctly.

use crate::support::*;

/// Issue #602: an uncaught `count()` on the sentinel-carrying Mixed cell must print the PHP
/// `TypeError` fatal to stderr and exit with failure instead of crashing (was: SIGSEGV 139).
#[test]
fn test_mixed_count_null_container_sentinel_throws_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
echo count($r), "\n";
"#,
    );
    assert!(
        !out.success,
        "expected the uncaught count() TypeError to exit with failure, stdout={:?} stderr={:?}",
        out.stdout, out.stderr
    );
    assert!(
        out.stderr.contains(
            "Uncaught TypeError: count(): Argument #1 ($value) must be of type Countable|array, null given"
        ),
        "missing the count() TypeError fatal, stderr={:?}",
        out.stderr
    );
    assert!(
        out.stderr.contains("Warning: Undefined array key 5"),
        "missing the missed-read warning, stderr={:?}",
        out.stderr
    );
    assert!(out.stdout.is_empty(), "count() must not print a number, stdout={:?}", out.stdout);
}

/// Issue #602: the same error is a catchable `TypeError`, matching the concrete-array path;
/// after catching, execution continues normally.
#[test]
fn test_mixed_count_null_container_sentinel_is_catchable() {
    let out = compile_and_run_capture(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
try {
    echo count($r), "\n";
} catch (TypeError $e) {
    echo "caught: " . $e->getMessage() . "\n";
}
echo "after\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "caught: count(): Argument #1 ($value) must be of type Countable|array, null given\nafter\n"
    );
    assert!(
        out.stderr.contains("Warning: Undefined array key 5"),
        "missing the missed-read warning, stderr={:?}",
        out.stderr
    );
}

/// Issue #602 boundary: only the null-container *sentinel* raises the `TypeError`. A Mixed
/// cell holding a real null value (tag 8, from `json_decode("null")`) keeps the legacy quiet
/// `0`, matching the codebase's off-web `count($_SERVER) == 0` convention (a plain null
/// pointer). Full PHP parity for real null (which raises a `TypeError`) is tracked with the
/// non-container-scalar divergence in issue #617; this test locks the current behavior so the
/// #602 sentinel fix does not silently change it.
#[test]
fn test_mixed_count_real_null_cell_is_legacy_zero() {
    let out = compile_and_run_capture(r#"<?php echo count(json_decode("null")), "\n";"#);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0\n");
}

/// Issue #602 control: when the same ternary merge delivers a real array, `count()` returns
/// the correct length and the guard does not fire.
#[test]
fn test_mixed_count_real_array_via_ternary_merge() {
    let out = compile_and_run_capture(
        r#"<?php
$rows = [[1, 2, 3]];
$r = $argc == 1 ? $rows[0] : ["a", "b"];
echo count($r), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "3\n");
}

/// Issue #602 control: an empty container in a Mixed cell still counts as `0`. The sentinel
/// signal (`NULL_SENTINEL`) must not collide with a legitimate zero count.
#[test]
fn test_mixed_count_empty_array_cell_is_zero() {
    let out = compile_and_run_capture(r#"<?php echo count(json_decode("[]")), "\n";"#);
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "0\n");
}

/// Issue #602: the catchable error path is leak-free under heap debug — the sentinel-carrying
/// Mixed cell is released cleanly when execution continues past the caught `TypeError`.
#[test]
fn test_mixed_count_null_container_sentinel_error_path_is_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
try {
    echo count($r), "\n";
} catch (TypeError $e) {
    echo "caught\n";
}
echo "after\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert!(out.stdout.contains("caught"), "expected the TypeError to be caught, stdout={:?}", out.stdout);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap on the count() error path, stderr={}",
        out.stderr
    );
}
