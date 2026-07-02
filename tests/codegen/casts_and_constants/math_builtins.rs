//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection math builtins, including pow operator, pow operator float, and pow right associative.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies the `**` exponentiation operator with integer base 2 and exponent 10: expects `1024`.
#[test]
fn test_pow_operator() {
    let out = compile_and_run("<?php echo 2 ** 10;");
    assert_eq!(out, "1024");
}

/// Verifies the `**` exponentiation operator with float operands 2.0 and 0.5, which equals sqrt(2): expects `1.4142135623731`.
#[test]
fn test_pow_operator_float() {
    let out = compile_and_run("<?php echo 2.0 ** 0.5;");
    assert_eq!(out, "1.4142135623731");
}

/// Verifies exponentiation is right-associative: `2 ** 3 ** 2` means `2 ** (3 ** 2)` = `2 ** 9` = `512`.
#[test]
fn test_pow_right_associative() {
    let out = compile_and_run("<?php echo 2 ** 3 ** 2;");
    assert_eq!(out, "512");
}

/// Verifies exponentiation has higher precedence than unary minus: `-2 ** 2` = `-(2**2)` = `-4`.
#[test]
fn test_pow_higher_than_unary() {
    let out = compile_and_run("<?php echo -2 ** 2;");
    assert_eq!(out, "-4");
}

/// Verifies exponentiation has higher precedence than multiplication: `3 * 2 ** 3` = `3 * 8` = `24`.
#[test]
fn test_pow_higher_than_multiply() {
    let out = compile_and_run("<?php echo 3 * 2 ** 3;");
    assert_eq!(out, "24");
}

// --- fmod, fdiv ---

/// Verifies `fmod(10.5, 3.2)` returns the floating-point remainder: expects `0.9`.
#[test]
fn test_fmod() {
    let out = compile_and_run("<?php echo fmod(10.5, 3.2);");
    assert_eq!(out, "0.9");
}

/// Verifies `fdiv(10, 3)` performs floating-point division: expects `3.3333333333333`.
#[test]
fn test_fdiv() {
    let out = compile_and_run("<?php echo fdiv(10, 3);");
    assert_eq!(out, "3.3333333333333");
}

/// Verifies `fdiv(1, 0)` returns `INF` instead of crashing on division by zero.
#[test]
fn test_fdiv_by_zero() {
    let out = compile_and_run("<?php echo fdiv(1, 0);");
    assert_eq!(out, "INF");
}

// --- rand, mt_rand, random_int ---

/// Verifies `rand(1, 1)` returns the degenerate single-value range: expects `1`.
#[test]
fn test_rand_range() {
    let out = compile_and_run("<?php echo rand(1, 1);");
    assert_eq!(out, "1");
}

/// Verifies `mt_rand(5, 5)` returns the degenerate single-value range: expects `5`.
#[test]
fn test_mt_rand_range() {
    let out = compile_and_run("<?php echo mt_rand(5, 5);");
    assert_eq!(out, "5");
}

/// Verifies `random_int(42, 42)` returns the degenerate single-value range: expects `42`.
#[test]
fn test_random_int_range() {
    let out = compile_and_run("<?php echo random_int(42, 42);");
    assert_eq!(out, "42");
}

/// Verifies `rand()` with no arguments does not crash and returns a non-negative integer.
#[test]
fn test_rand_no_args() {
    let out = compile_and_run("<?php $r = rand(); echo ($r >= 0 ? \"ok\" : \"bad\");");
    assert_eq!(out, "ok");
}

// --- number_format ---

/// Verifies `number_format(1234567)` formats with default 0 decimals, comma thousands separator: expects `1,234,567`.
#[test]
fn test_number_format_no_decimals() {
    let out = compile_and_run("<?php echo number_format(1234567);");
    assert_eq!(out, "1,234,567");
}

/// Verifies `number_format(1234.5678, 2)` rounds to 2 decimal places: expects `1,234.57`.
#[test]
fn test_number_format_with_decimals() {
    let out = compile_and_run("<?php echo number_format(1234.5678, 2);");
    assert_eq!(out, "1,234.57");
}

/// Verifies `number_format(42, 2)` pads small numbers to 2 decimal places: expects `42.00`.
#[test]
fn test_number_format_small() {
    let out = compile_and_run("<?php echo number_format(42, 2);");
    assert_eq!(out, "42.00");
}

/// Verifies `number_format(-1234.5, 1)` handles negative numbers: expects `-1,234.5`.
#[test]
fn test_number_format_negative() {
    let out = compile_and_run("<?php echo number_format(-1234.5, 1);");
    assert_eq!(out, "-1,234.5");
}

/// Verifies `number_format` with custom decimal `,` and thousands `.` separators (European style): expects `1.234.567,89`.
#[test]
fn test_number_format_custom_separators() {
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ",", ".");"#);
    assert_eq!(out, "1.234.567,89");
}

/// Verifies `number_format` with empty string as thousands separator disables grouping: expects `1234567.89`.
#[test]
fn test_number_format_no_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567.89, 2, ".", "");"#);
    assert_eq!(out, "1234567.89");
}

/// Verifies `number_format` with a space as thousands separator: expects `1 234 567`.
#[test]
fn test_number_format_space_thousands() {
    let out = compile_and_run(r#"<?php echo number_format(1234567, 0, ".", " ");"#);
    assert_eq!(out, "1 234 567");
}

// --- random_bytes ---

/// Verifies `random_bytes(16)` returns a 16-byte binary string (constant length).
#[test]
fn test_random_bytes_length() {
    let out = compile_and_run("<?php echo strlen(random_bytes(16));");
    assert_eq!(out, "16");
}

/// Verifies `random_bytes()` honors a runtime-unknown length (via `$argc`) so the
/// dynamic-length runtime path is exercised rather than a folded constant.
#[test]
fn test_random_bytes_runtime_length() {
    let out = compile_and_run("<?php echo strlen(random_bytes(32 + $argc - $argc));");
    assert_eq!(out, "32");
}

/// Verifies a fully-qualified `\random_bytes()` call resolves through namespace
/// fallback and still returns the requested number of bytes.
#[test]
fn test_random_bytes_namespaced() {
    let out = compile_and_run("<?php echo strlen(\\random_bytes(8));");
    assert_eq!(out, "8");
}

/// Verifies PHP case-insensitive builtin lookup: `RANDOM_BYTES(8)` resolves to
/// `random_bytes` and returns 8 bytes.
#[test]
fn test_random_bytes_case_insensitive() {
    let out = compile_and_run("<?php echo strlen(RANDOM_BYTES(8));");
    assert_eq!(out, "8");
}

/// Verifies two `random_bytes(16)` results differ (guards against the impure call
/// being constant-folded or deduplicated): `var_dump` shows `bool(true)`.
#[test]
fn test_random_bytes_distinct() {
    let out = compile_and_run("<?php var_dump(random_bytes(16) !== random_bytes(16));");
    assert_eq!(out, "bool(true)\n");
}

// --- Constants ---
