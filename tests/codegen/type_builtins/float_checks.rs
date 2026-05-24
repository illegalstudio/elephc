//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins float checking builtins, including inf constant, nan constant, and negative inf.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
    // Compiles `<?php echo INF;` and asserts stdout is `INF`.
fn test_inf_constant() {
    let out = compile_and_run("<?php echo INF;");
    assert_eq!(out, "INF");
}

#[test]
    // Compiles `<?php echo NAN;` and asserts stdout is `NAN`.
fn test_nan_constant() {
    let out = compile_and_run("<?php echo NAN;");
    assert_eq!(out, "NAN");
}

#[test]
    // Compiles `<?php echo -INF;` and asserts stdout is `-INF`, verifying negation of the INF constant.
fn test_negative_inf() {
    let out = compile_and_run("<?php echo -INF;");
    assert_eq!(out, "-INF");
}

#[test]
    // Compiles `<?php echo is_nan(NAN);` and asserts stdout is `1`, confirming is_nan() returns true for NAN.
fn test_is_nan_true() {
    let out = compile_and_run("<?php echo is_nan(NAN);");
    assert_eq!(out, "1");
}

#[test]
    // Compiles `<?php echo is_nan(42.0);` and asserts stdout is empty (PHP false is empty string), confirming is_nan() returns false for a regular float.
fn test_is_nan_false() {
    let out = compile_and_run("<?php echo is_nan(42.0);");
    assert_eq!(out, "");
}

#[test]
    // Compiles `<?php echo is_nan(0);` and asserts stdout is empty, confirming is_nan() returns false for an integer without float coercion.
fn test_is_nan_int() {
    let out = compile_and_run("<?php echo is_nan(0);");
    assert_eq!(out, "");
}

#[test]
    // Compiles `<?php echo is_infinite(INF);` and asserts stdout is `1`, confirming is_infinite() returns true for positive INF.
fn test_is_infinite_true() {
    let out = compile_and_run("<?php echo is_infinite(INF);");
    assert_eq!(out, "1");
}

#[test]
    // Compiles `<?php echo is_infinite(-INF);` and asserts stdout is `1`, confirming is_infinite() returns true for negative INF.
fn test_is_infinite_neg_inf() {
    let out = compile_and_run("<?php echo is_infinite(-INF);");
    assert_eq!(out, "1");
}

#[test]
    // Compiles `<?php echo is_infinite(42.0);` and asserts stdout is empty (PHP false is empty string), confirming is_infinite() returns false for a regular float.
fn test_is_infinite_false() {
    let out = compile_and_run("<?php echo is_infinite(42.0);");
    assert_eq!(out, "");
}

#[test]
    // Compiles `<?php echo is_finite(42.0);` and asserts stdout is `1`, confirming is_finite() returns true for a regular float.
fn test_is_finite_true() {
    let out = compile_and_run("<?php echo is_finite(42.0);");
    assert_eq!(out, "1");
}

#[test]
    // Compiles `<?php echo is_finite(INF);` and asserts stdout is empty (PHP false is empty string), confirming is_finite() returns false for INF.
fn test_is_finite_inf() {
    let out = compile_and_run("<?php echo is_finite(INF);");
    assert_eq!(out, "");
}

#[test]
    // Compiles `<?php echo is_finite(NAN);` and asserts stdout is empty (PHP false is empty string), confirming is_finite() returns false for NAN.
fn test_is_finite_nan() {
    let out = compile_and_run("<?php echo is_finite(NAN);");
    assert_eq!(out, "");
}

#[test]
    // Compiles `<?php echo INF + 1;` and asserts stdout is `INF`, confirming INF propagates through addition.
fn test_inf_arithmetic() {
    let out = compile_and_run("<?php echo INF + 1;");
    assert_eq!(out, "INF");
}
