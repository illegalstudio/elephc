//! Purpose:
//! Integration tests for the EIR common-subexpression elimination pass (`cse`)
//! driven by the fixed-point pass driver. Fixtures use a local seeded from the
//! runtime `$argc` value so the repeated subexpressions are not constant-folded
//! away at the AST level and reach EIR, where peephole load forwarding collapses
//! the repeated reads to one value and CSE then deduplicates the identical pure
//! computations built on it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `$argc` is 1 with no CLI arguments, so each fixture's expected output is the
//!   value the un-deduplicated program would also compute — CSE must be behavior
//!   preserving.

use super::*;

/// A repeated `$n + 1` subexpression is computed once: `($n + 1) * ($n + 1)` with
/// `$n == 1` is `4`.
#[test]
fn test_cse_repeated_arithmetic() {
    let out = compile_and_run("<?php $n = $argc; echo ($n + 1) * ($n + 1);");
    assert_eq!(out, "4");
}

/// A larger common subexpression `$n * 3 + 7` is shared across the addition: with
/// `$n == 1` the result is `(3 + 7) + (3 + 7) == 20`.
#[test]
fn test_cse_larger_common_subexpression() {
    let out = compile_and_run("<?php $n = $argc; echo ($n * 3 + 7) + ($n * 3 + 7);");
    assert_eq!(out, "20");
}

/// A repeated comparison `$n > 5` is computed once and reused by both the
/// assignment and the conditional; with `$n == 1` the branch is the false arm.
#[test]
fn test_cse_repeated_comparison() {
    let out = compile_and_run("<?php $n = $argc; $x = $n > 5; echo ($n > 5) ? \"a\" : \"b\";");
    assert_eq!(out, "b");
}

/// CSE must not change behavior when the operands differ: `($n + 1) * ($n + 2)`
/// has two distinct subexpressions and stays `2 * 3 == 6` for `$n == 1`.
#[test]
fn test_cse_distinct_subexpressions_preserved() {
    let out = compile_and_run("<?php $n = $argc; echo ($n + 1) * ($n + 2);");
    assert_eq!(out, "6");
}
