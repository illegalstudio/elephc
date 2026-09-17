//! Purpose:
//! Heap-debug coverage for boxing a `TaggedScalar` into a `Mixed` cell at a call boundary.
//! A nullable-int union is the one union codegen stores unboxed (payload register plus tag
//! register), so handing one to a `mixed` parameter has to allocate a real cell -- and the
//! caller has to release that cell again once the call returns.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`, so a cell the
//!   caller boxes but never releases shows up as one leaked block per call.
//! - The loops run enough iterations that a per-call leak cannot hide inside the heap's slack:
//!   a single unreleased block per iteration is several hundred blocks by the last one.
//! - Expected stdout values are real `LC_ALL=C php` 8.5 output for the same fixtures.

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

/// Boxing a nullable-int argument allocates a cell the caller still has to release.
///
/// Before issue #1046 this path emitted no boxing at all, so it also allocated nothing; the
/// fix makes every such call allocate, which is exactly the shape that leaks if the existing
/// argument-temporary cleanup does not cover it.
#[test]
fn test_tagged_scalar_argument_boxing_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function p($v) { return is_int($v) ? 1 : 0; }
$e = [];
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $t += p(count($e) > 0 ? $e[0] : 7);
}
echo $t;
"#,
    );
    assert_clean(out, "300");
}

/// The null arm allocates too: `__rt_mixed_from_value` boxes a null tag into its own cell.
///
/// Covered separately from the integer arm because the two take different runtime tags, and a
/// release keyed off the payload rather than the cell would balance one and not the other.
#[test]
fn test_tagged_scalar_null_argument_boxing_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function p($v) { return $v === null ? 1 : 0; }
$e = [];
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $t += p(count($e) > 0 ? 1 : null);
}
echo $t;
"#,
    );
    assert_clean(out, "300");
}

/// The empty-spread form the issue reported stays balanced across repeated calls.
///
/// Each parameter gets its own default ternary, so a two-parameter call boxes twice per
/// iteration; this pins that both cells are released, not just the first.
#[test]
fn test_empty_spread_default_boxing_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f($a = 0, $b = 0) { return $a + $b; }
$e = [];
$t = 0;
for ($i = 0; $i < 300; $i++) {
    $t += f(...$e);
}
echo $t;
"#,
    );
    assert_clean(out, "0");
}
