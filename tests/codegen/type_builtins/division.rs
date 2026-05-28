//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins division, including integer division returns float, integer division exact, and division assign updates type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `/` produces a float-formatted string (PHP semantics: non-integer division returns float).
#[test]
fn test_int_division_returns_float() {
    let out = compile_and_run("<?php echo 10 / 3;");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies exact division still returns float-formatted output, not integer.
#[test]
fn test_int_division_exact() {
    let out = compile_and_run("<?php echo 10 / 2;");
    assert_eq!(out, "5");
}

/// Verifies compound assignment `/=` updates the variable type to float.
#[test]
fn test_division_assign_updates_type() {
    let out = compile_and_run("<?php $x = 10; $x /= 3; echo $x;");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies float arithmetic is used when summing multiple division results.
#[test]
fn test_division_in_expression() {
    let out = compile_and_run("<?php echo 1 / 3 + 1 / 3 + 1 / 3;");
    assert_eq!(out, "1");
}

/// Verifies `intdiv()` returns an integer (truncates toward zero).
#[test]
fn test_intdiv_still_returns_int() {
    let out = compile_and_run("<?php echo intdiv(10, 3);");
    assert_eq!(out, "3");
}

/// Verifies `intdiv()` with exact division returns integer without decimal.
#[test]
fn test_intdiv_exact() {
    let out = compile_and_run("<?php echo intdiv(10, 5);");
    assert_eq!(out, "2");
}

/// Verifies `intdiv()` with negative dividend truncates toward zero (not floor).
#[test]
fn test_intdiv_negative() {
    let out = compile_and_run("<?php echo intdiv(-7, 2);");
    assert_eq!(out, "-3");
}

/// Verifies float division by zero produces `INF`.
#[test]
fn test_division_by_zero_inf() {
    let out = compile_and_run("<?php echo 1.0 / 0.0;");
    assert_eq!(out, "INF");
}
