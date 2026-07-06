//! Purpose:
//! Integration or regression tests for diagnostic coverage of math builtins, including floor wrong args, ceil wrong args, and round wrong args.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies floor() rejects excess positional arguments. Input: `floor(1, 2)`.
#[test]
fn test_error_floor_wrong_args() {
    expect_error("<?php floor(1, 2);", "floor() takes exactly 1 argument");
}

/// Verifies ceil() rejects missing argument. Input: `ceil()` with no args.
#[test]
fn test_error_ceil_wrong_args() {
    expect_error("<?php ceil();", "ceil() takes exactly 1 argument");
}

/// Verifies round() rejects missing argument. Input: `round()` with no args.
#[test]
fn test_error_round_wrong_args() {
    expect_error("<?php round();", "round() takes 1 to 3 arguments");
}

/// Verifies round() rejects the not-yet-specialized HALF_DOWN/HALF_ODD modes.
///
/// Input: `round(1.5, 0, PHP_ROUND_HALF_DOWN)` — only PHP_ROUND_HALF_UP and PHP_ROUND_HALF_EVEN
/// are supported.
#[test]
fn test_error_round_unsupported_mode() {
    expect_error(
        "<?php round(1.5, 0, PHP_ROUND_HALF_DOWN);",
        "only PHP_ROUND_HALF_UP and PHP_ROUND_HALF_EVEN modes are supported",
    );
}

/// Verifies sqrt() rejects excess positional arguments. Input: `sqrt(1, 2)`.
#[test]
fn test_error_sqrt_wrong_args() {
    expect_error("<?php sqrt(1, 2);", "sqrt() takes exactly 1 argument");
}

/// Verifies pow() rejects missing second argument. Input: `pow(1)` with only one arg.
#[test]
fn test_error_pow_wrong_args() {
    expect_error("<?php pow(1);", "pow() takes exactly 2 arguments");
}

/// Verifies min() rejects single-argument call (requires at least 2). Input: `min(1)`.
#[test]
fn test_error_min_wrong_args() {
    expect_error("<?php min(1);", "min() requires at least 2 arguments");
}

/// Verifies max() rejects single-argument call (requires at least 2). Input: `max(1)`.
#[test]
fn test_error_max_wrong_args() {
    expect_error("<?php max(1);", "max() requires at least 2 arguments");
}

/// Verifies clamp() rejects missing bound arguments. Input: `clamp(1, 2)`.
#[test]
fn test_error_clamp_wrong_args() {
    expect_error("<?php clamp(1, 2);", "clamp() takes exactly 3 arguments");
}

/// Verifies intdiv() rejects missing second argument. Input: `intdiv(1)` with only one arg.
#[test]
fn test_error_intdiv_wrong_args() {
    expect_error("<?php intdiv(1);", "intdiv() takes exactly 2 arguments");
}

/// Verifies abs() rejects missing argument. Input: `abs()` with no args.
#[test]
fn test_error_abs_wrong_args() {
    expect_error("<?php abs();", "abs() takes exactly 1 argument");
}

/// Verifies floatval() rejects missing argument. Input: `floatval()` with no args.
#[test]
fn test_error_floatval_wrong_args() {
    expect_error("<?php floatval();", "floatval() takes exactly 1 argument");
}

/// Verifies is_float() rejects missing argument. Input: `is_float()` with no args.
#[test]
fn test_error_is_float_wrong_args() {
    expect_error("<?php is_float();", "is_float() takes exactly 1 argument");
}

/// Verifies is_int() rejects missing argument. Input: `is_int()` with no args.
#[test]
fn test_error_is_int_wrong_args() {
    expect_error("<?php is_int();", "is_int() takes exactly 1 argument");
}

/// Verifies is_nan() rejects missing argument. Input: `is_nan()` with no args.
#[test]
fn test_error_is_nan_wrong_args() {
    expect_error("<?php is_nan();", "is_nan() takes exactly 1 argument");
}

/// Verifies is_finite() rejects missing argument. Input: `is_finite()` with no args.
#[test]
fn test_error_is_finite_wrong_args() {
    expect_error("<?php is_finite();", "is_finite() takes exactly 1 argument");
}

/// Verifies is_infinite() rejects missing argument. Input: `is_infinite()` with no args.
#[test]
fn test_error_is_infinite_wrong_args() {
    expect_error(
        "<?php is_infinite();",
        "is_infinite() takes exactly 1 argument",
    );
}

// --- Type operation errors ---

/// Verifies fmod() rejects missing second argument. Input: `fmod(1)` with only one arg.
#[test]
fn test_error_fmod_wrong_args() {
    expect_error("<?php fmod(1);", "fmod() takes exactly 2 arguments");
}

/// Verifies random_int() rejects missing second argument. Input: `random_int(1)` with only one arg.
#[test]
fn test_error_random_int_wrong_args() {
    expect_error(
        "<?php random_int(1);",
        "random_int() takes exactly 2 arguments",
    );
}

/// Verifies number_format() rejects missing argument. Input: `number_format()` with no args.
#[test]
fn test_error_number_format_wrong_args() {
    expect_error(
        "<?php number_format();",
        "number_format() takes 1 to 4 arguments",
    );
}

// --- String function errors ---
