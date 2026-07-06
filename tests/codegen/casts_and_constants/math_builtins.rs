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

/// Verifies `number_format` rounds exact-half cases away from zero (PHP `PHP_ROUND_HALF_UP`), not
/// half-to-even like a bare `snprintf("%.*f")`: `2.5 -> 3`, `0.5 -> 1`, `-2.5 -> -3`, `2.55 -> 2.6`.
#[test]
fn test_number_format_rounds_half_away_from_zero() {
    let out = compile_and_run(
        r#"<?php
echo number_format(2.5, 0); echo ",";
echo number_format(3.5, 0); echo ",";
echo number_format(0.5, 0); echo ",";
echo number_format(1.5, 0); echo ",";
echo number_format(-2.5, 0); echo ",";
echo number_format(2.45, 1); echo ",";
echo number_format(2.55, 1); echo ",";
echo number_format(1234.5, 0);
"#,
    );
    assert_eq!(out, "3,4,1,2,-3,2.5,2.6,1,235");
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

/// Verifies a multi-byte (UTF-8 non-breaking space, `\xc2\xa0`) thousands separator survives intact
/// rather than being truncated to its first byte, matching PHP's `fr_FR`-style grouping.
#[test]
fn test_number_format_multibyte_thousands_separator() {
    let out = compile_and_run(
        "<?php echo number_format(1234567.891, 2, \",\", \"\\xc2\\xa0\");",
    );
    assert_eq!(out, "1\u{a0}234\u{a0}567,89");
}

/// Verifies a multi-byte decimal separator is inserted whole (both separators are `(ptr, len)`
/// strings now, not single bytes).
#[test]
fn test_number_format_multibyte_decimal_separator() {
    let out = compile_and_run(r#"<?php echo number_format(1234.5, 2, "__", " ");"#);
    assert_eq!(out, "1 234__50");
}

/// Verifies an empty decimal AND thousands separator both pass through as length-0 "insert nothing"
/// (the multi-byte helper treats length 0 as no separator, not a truncated single byte).
#[test]
fn test_number_format_empty_both_separators() {
    let out = compile_and_run(r#"<?php echo number_format(1234567.891, 2, "", "");"#);
    assert_eq!(out, "123456789");
}

// --- Constants ---
