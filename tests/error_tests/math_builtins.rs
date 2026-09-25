//! Purpose:
//! Integration or regression tests for diagnostic coverage of math builtins, including BCMath,
//! floor, ceil, and round argument-count failures.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies `bcadd()` rejects a call missing its second required operand.
#[test]
fn test_error_bcadd_too_few_args() {
    expect_error("<?php bcadd('1');", "bcadd() takes 2 or 3 arguments");
}

/// Verifies `bcpowmod()` rejects a call missing its modulus operand.
#[test]
fn test_error_bcpowmod_too_few_args() {
    expect_error(
        "<?php bcpowmod('2', '3');",
        "bcpowmod() takes 3 or 4 arguments",
    );
}

/// Verifies `bcscale()` rejects more than its single optional scale argument.
#[test]
fn test_error_bcscale_too_many_args() {
    expect_error("<?php bcscale(1, 2);", "bcscale() takes at most 1 argument");
}

/// Verifies the one-argument BCMath rounding helpers reject missing operands.
#[test]
fn test_error_bcmath_single_operand_arities() {
    expect_error("<?php bcceil();", "bcceil() takes exactly 1 argument");
    expect_error("<?php bcfloor();", "bcfloor() takes exactly 1 argument");
}

/// Verifies the BCMath binary helpers reject calls missing their second operand.
#[test]
fn test_error_bcmath_binary_arities() {
    for name in ["bccomp", "bcdiv", "bcdivmod", "bcmod", "bcmul", "bcpow", "bcsub"] {
        expect_error(
            &format!("<?php {name}('1');"),
            &format!("{name}() takes 2 or 3 arguments"),
        );
    }
}

/// Verifies BCMath helpers with one required operand reject missing input.
#[test]
fn test_error_bcmath_optional_argument_arities() {
    expect_error("<?php bcround();", "bcround() takes 1 to 3 arguments");
    expect_error("<?php bcsqrt();", "bcsqrt() takes 1 or 2 arguments");
}

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
///
/// `round()` gained PHP 8.4's third `$mode` parameter, so the arity diagnostic now spans
/// `1 to 3` arguments.
#[test]
fn test_error_round_wrong_args() {
    expect_error("<?php round();", "round() takes 1 to 3 arguments");
}

/// Verifies round() rejects a fourth argument. Input: `round(1.0, 2, 3, 4)`.
#[test]
fn test_error_round_too_many_args() {
    expect_error("<?php echo round(1.0, 2, 3, 4);", "round() takes 1 to 3 arguments");
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

/// Verifies min() rejects a lone non-array argument with php-src's TypeError wording.
/// PHP's single-argument form takes an array; `min(1)` is a TypeError there too.
#[test]
fn test_error_min_wrong_args() {
    expect_error(
        "<?php min(1);",
        "min(): Argument #1 ($value) must be of type array, int given",
    );
}

/// Verifies max() rejects a lone non-array argument with php-src's TypeError wording.
#[test]
fn test_error_max_wrong_args() {
    expect_error(
        "<?php max(1);",
        "max(): Argument #1 ($value) must be of type array, int given",
    );
}

/// Verifies min() with no argument at all still reports PHP's ArgumentCountError text.
#[test]
fn test_error_min_no_args() {
    expect_error("<?php min();", "min() expects at least 1 argument, 0 given");
}

/// Verifies max() with no argument at all still reports PHP's ArgumentCountError text.
#[test]
fn test_error_max_no_args() {
    expect_error("<?php max();", "max() expects at least 1 argument, 0 given");
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

/// Verifies random_bytes() rejects a missing argument. Input: `random_bytes()`.
#[test]
fn test_error_random_bytes_no_args() {
    expect_error(
        "<?php random_bytes();",
        "random_bytes() takes exactly 1 argument",
    );
}

/// Verifies random_bytes() rejects excess arguments. Input: `random_bytes(1, 2)`.
#[test]
fn test_error_random_bytes_too_many_args() {
    expect_error(
        "<?php random_bytes(1, 2);",
        "random_bytes() takes exactly 1 argument",
    );
}

/// Verifies random_bytes() rejects a constant length of 0 at compile time. Input: `random_bytes(0)`.
#[test]
fn test_error_random_bytes_zero_length() {
    expect_error(
        "<?php random_bytes(0);",
        "random_bytes(): Argument #1 ($length) must be greater than 0",
    );
}

/// Verifies random_bytes() rejects a constant negative length at compile time. Input: `random_bytes(-1)`.
#[test]
fn test_error_random_bytes_negative_length() {
    expect_error(
        "<?php random_bytes(-1);",
        "random_bytes(): Argument #1 ($length) must be greater than 0",
    );
}

// --- String function errors ---

/// Verifies that `dechex()` with no arguments produces the correct arity error.
#[test]
fn test_error_dechex_wrong_args() {
    expect_error("<?php dechex();", "dechex() takes exactly 1 argument");
}

/// Verifies that `decbin()` with two arguments produces the correct arity error.
#[test]
fn test_error_decbin_too_many_args() {
    expect_error("<?php decbin(1, 2);", "decbin() takes exactly 1 argument");
}

/// Verifies that `decoct()` with no arguments produces the correct arity error.
#[test]
fn test_error_decoct_wrong_args() {
    expect_error("<?php decoct();", "decoct() takes exactly 1 argument");
}

/// Verifies that `hexdec()` with no arguments produces the correct arity error.
#[test]
fn test_error_hexdec_wrong_args() {
    expect_error("<?php hexdec();", "hexdec() takes exactly 1 argument");
}

/// Verifies that `bindec()` with two arguments produces the correct arity error.
#[test]
fn test_error_bindec_too_many_args() {
    expect_error("<?php bindec(\"1\", 2);", "bindec() takes exactly 1 argument");
}

/// Verifies that `octdec()` with no arguments produces the correct arity error.
#[test]
fn test_error_octdec_wrong_args() {
    expect_error("<?php octdec();", "octdec() takes exactly 1 argument");
}

/// Verifies that `base_convert()` with two arguments produces the correct arity error.
#[test]
fn test_error_base_convert_wrong_args() {
    expect_error(
        "<?php base_convert(\"ff\", 16);",
        "base_convert() takes exactly 3 arguments",
    );
}

/// Verifies that `base_convert()` with four arguments produces the correct arity error.
#[test]
fn test_error_base_convert_too_many_args() {
    expect_error(
        "<?php base_convert(\"ff\", 16, 10, 2);",
        "base_convert() takes exactly 3 arguments",
    );
}
