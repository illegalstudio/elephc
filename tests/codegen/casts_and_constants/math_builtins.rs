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

/// Verifies `random_int(0, PHP_INT_MAX)` actually samples the 63-bit range.
///
/// The helper used to take its bound in a 32-bit register, so a span of 2^63
/// truncated to zero and every draw returned the lower bound. The degenerate-range
/// tests above could not see it: they only ever asked for a single-value range.
/// Drawing eight times and requiring one non-zero makes a false failure a
/// 2^-504 event.
#[test]
fn test_random_int_spans_full_int_range() {
    let out = compile_and_run(
        "<?php $hit = 0; for ($i = 0; $i < 8; $i++) { $v = random_int(0, PHP_INT_MAX); \
         if ($v < 0 || $v > PHP_INT_MAX) { echo 'range'; return; } if ($v !== 0) { $hit++; } } \
         echo $hit > 0 ? 'ok' : 'always-zero';",
    );
    assert_eq!(out, "ok");
}

/// Verifies the 32-bit/64-bit sampling boundary: a span of exactly 2^32 must not
/// collapse. `random_int(0, 4294967295)` has 2^32 admissible values, one past what
/// a uint32 exclusive bound can express, and used to return zero every time.
#[test]
fn test_random_int_spans_uint32_boundary() {
    let out = compile_and_run(
        "<?php $hit = 0; for ($i = 0; $i < 8; $i++) { $v = random_int(0, 4294967295); \
         if ($v < 0 || $v > 4294967295) { echo 'range'; return; } if ($v !== 0) { $hit++; } } \
         echo $hit > 0 ? 'ok' : 'always-zero';",
    );
    assert_eq!(out, "ok");
}

/// Verifies the widest possible range stays in bounds. `PHP_INT_MAX - PHP_INT_MIN`
/// is `UINT64_MAX`, so forming an exclusive bound would wrap to zero; the helper
/// takes an inclusive width precisely to keep this case representable.
#[test]
fn test_random_int_full_width_range_stays_in_bounds() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 8; $i++) { $v = random_int(PHP_INT_MIN, PHP_INT_MAX); \
         if ($v < PHP_INT_MIN || $v > PHP_INT_MAX) { echo 'range'; return; } } echo 'ok';",
    );
    assert_eq!(out, "ok");
}

/// Verifies `mt_rand()` over a wide range is sampled with the same 64-bit path,
/// since `rand`, `mt_rand` and `random_int` share one lowering.
#[test]
fn test_mt_rand_wide_range_is_not_always_zero() {
    let out = compile_and_run(
        "<?php $hit = 0; for ($i = 0; $i < 8; $i++) { if (mt_rand(0, PHP_INT_MAX) !== 0) { $hit++; } } \
         echo $hit > 0 ? 'ok' : 'always-zero';",
    );
    assert_eq!(out, "ok");
}

/// Verifies a negative inclusive range remains in bounds and both endpoints are
/// reachable, covering the `min` re-addition after the unsigned sampling step.
#[test]
fn test_random_int_negative_range_in_bounds() {
    let out = compile_and_run(
        "<?php $lo = false; $hi = false; for ($i = 0; $i < 200; $i++) { $v = random_int(-1, 0); \
         if ($v < -1 || $v > 0) { echo 'range'; return; } if ($v === -1) { $lo = true; } \
         if ($v === 0) { $hi = true; } } echo ($lo && $hi) ? 'ok' : 'skewed';",
    );
    assert_eq!(out, "ok");
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

/// Verifies PHP's decimal pre-rounding fixes binary representation edges before formatting.
#[test]
fn test_number_format_php_rounding_edges() {
    let out = compile_and_run(
        "<?php echo number_format(1.005, 2), '|', number_format(-0.01, 0), '|', number_format(999.995, 2);",
    );
    assert_eq!(out, "1.01|0|1,000.00");
}

/// Verifies multi-digit precisions preserve PHP's binary-double formatting
/// semantics instead of treating the precision as one malformed ASCII digit.
#[test]
fn test_number_format_high_precision_php_corpus() {
    let out = compile_and_run(
        r#"<?php
echo number_format(0.285, 23 + $argc - $argc, ".", ""), "|";
echo number_format(1.2345678901234567, 23, ".", ""), "|";
echo number_format(0.285, 30, ".", "");"#,
    );
    assert_eq!(
        out,
        "0.28499999999999997557509|1.23456789012345669043214|0.284999999999999975575093458247"
    );
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
