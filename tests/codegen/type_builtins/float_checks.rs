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
fn test_inf_constant() {
    let out = compile_and_run("<?php echo INF;");
    assert_eq!(out, "INF");
}

#[test]
fn test_nan_constant() {
    let out = compile_and_run("<?php echo NAN;");
    assert_eq!(out, "NAN");
}

#[test]
fn test_negative_inf() {
    let out = compile_and_run("<?php echo -INF;");
    assert_eq!(out, "-INF");
}

#[test]
fn test_is_nan_true() {
    let out = compile_and_run("<?php echo is_nan(NAN);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_nan_false() {
    let out = compile_and_run("<?php echo is_nan(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_nan_int() {
    let out = compile_and_run("<?php echo is_nan(0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_infinite_true() {
    let out = compile_and_run("<?php echo is_infinite(INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_neg_inf() {
    let out = compile_and_run("<?php echo is_infinite(-INF);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_infinite_false() {
    let out = compile_and_run("<?php echo is_infinite(42.0);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_true() {
    let out = compile_and_run("<?php echo is_finite(42.0);");
    assert_eq!(out, "1");
}

#[test]
fn test_is_finite_inf() {
    let out = compile_and_run("<?php echo is_finite(INF);");
    assert_eq!(out, "");
}

#[test]
fn test_is_finite_nan() {
    let out = compile_and_run("<?php echo is_finite(NAN);");
    assert_eq!(out, "");
}

#[test]
fn test_inf_arithmetic() {
    let out = compile_and_run("<?php echo INF + 1;");
    assert_eq!(out, "INF");
}
