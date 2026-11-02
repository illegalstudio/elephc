//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of math functions, including math trig basic, math trig pi, and math inverse trig.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Tests sin, cos, and tan with zero input — verifies correct float results with 4-decimal rounding.
#[test]
fn test_math_trig_basic() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(0.0), 4) . "|" . round(cos(0.0), 4) . "|" . round(tan(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

/// Tests sin, cos, and tan with known angle constants (M_PI_2, M_PI, M_PI_4) — verifies PHP math constant substitution and trig precision with 4-decimal rounding.
#[test]
fn test_math_trig_pi() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(M_PI_2), 4) . "|" . round(cos(M_PI), 1) . "|" . round(tan(M_PI_4), 4);
"#,
    );
    assert_eq!(out, "1|-1|1");
}

/// Tests asin, acos, and atan with boundary inputs (0, 1) — verifies inverse trig rounding to 4 decimals (1.5708 for 𝜋/2).
#[test]
fn test_math_inverse_trig() {
    let out = compile_and_run(
        r#"<?php
echo round(asin(1.0), 4) . "|" . round(acos(0.0), 4) . "|" . round(atan(1.0), 4);
"#,
    );
    assert_eq!(out, "1.5708|1.5708|0.7854");
}

/// Tests atan2 with (1.0, 0.0) input — verifies quadrant-aware arctan returning 𝜋/2 (1.5708).
#[test]
fn test_math_atan2() {
    let out = compile_and_run(
        r#"<?php
echo round(atan2(1.0, 0.0), 4);
"#,
    );
    assert_eq!(out, "1.5708");
}

/// Tests sinh, cosh, and tanh at zero input — verifies hyperbolic identity cosh(0)=1, sinh(0)=0, tanh(0)=0.
#[test]
fn test_math_hyperbolic() {
    let out = compile_and_run(
        r#"<?php
echo round(sinh(0.0), 4) . "|" . round(cosh(0.0), 4) . "|" . round(tanh(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

/// Tests log(M_E), log2(8), log10(1000), and exp(0) — verifies natural log, base-2, base-10, and exponential precision.
#[test]
fn test_math_log_exp() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4) . "|" . log2(8.0) . "|" . log10(1000.0) . "|" . exp(0.0);
"#,
    );
    assert_eq!(out, "1|3|3|1");
}

/// Tests hypot(3.0, 4.0) — verifies 3-4-5 right triangle result (5.0) for Euclidean distance.
#[test]
fn test_math_hypot() {
    let out = compile_and_run(
        r#"<?php
echo hypot(3.0, 4.0);
"#,
    );
    assert_eq!(out, "5");
}

/// Tests deg2rad(180.0) and rad2deg(M_PI) — verifies degree↔radian conversion (π rad = 180°).
#[test]
fn test_math_deg_rad() {
    let out = compile_and_run(
        r#"<?php
echo round(deg2rad(180.0), 4) . "|" . round(rad2deg(M_PI), 1);
"#,
    );
    assert_eq!(out, "3.1416|180");
}

/// Tests pi() function — verifies it returns a value rounding to 3.1416 at 4 decimals.
#[test]
fn test_math_pi_function() {
    let out = compile_and_run(
        r#"<?php
echo round(pi(), 4);
"#,
    );
    assert_eq!(out, "3.1416");
}

/// Tests M_E, M_SQRT2, M_PI_2, and M_PI_4 constants — verifies each rounds correctly (e≈2.7183, √2≈1.4142, 𝜋/2≈1.5708, 𝜋/4≈0.7854).
#[test]
fn test_math_constants() {
    let out = compile_and_run(
        r#"<?php
echo round(M_E, 4) . "|" . round(M_SQRT2, 4) . "|" . round(M_PI_2, 4) . "|" . round(M_PI_4, 4);
"#,
    );
    assert_eq!(out, "2.7183|1.4142|1.5708|0.7854");
}

/// Tests that integer literals passed to sin, cos, log, exp are coerced to float — verifies int→float coercion on argument materialization.
#[test]
fn test_math_int_coercion() {
    let out = compile_and_run(
        r#"<?php
echo sin(0) . "|" . cos(0) . "|" . log(1) . "|" . exp(0);
"#,
    );
    assert_eq!(out, "0|1|0|1");
}

/// Tests hypot with computed differences (4-1, 6-2) — verifies Euclidean distance with variable operands (expects 5.0).
#[test]
fn test_math_distance_calculation() {
    let out = compile_and_run(
        r#"<?php
$x1 = 1.0; $y1 = 2.0;
$x2 = 4.0; $y2 = 6.0;
$dist = hypot($x2 - $x1, $y2 - $y1);
echo round($dist, 4);
"#,
    );
    assert_eq!(out, "5");
}

/// Tests log(M_E) — verifies natural logarithm returns 1.0 (with 4-decimal rounding).
#[test]
fn test_log_natural() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4);
"#,
    );
    assert_eq!(out, "1");
}

/// Tests log(1000, 10) with explicit base argument — verifies base-10 logarithm returns 3.0.
#[test]
fn test_log_base_10() {
    let out = compile_and_run(
        r#"<?php
echo log(1000, 10);
"#,
    );
    assert_eq!(out, "3");
}

/// Tests log(256, 2) with explicit base argument — verifies base-2 logarithm returns 8.0.
#[test]
fn test_log_base_2() {
    let out = compile_and_run(
        r#"<?php
echo log(256, 2);
"#,
    );
    assert_eq!(out, "8");
}

/// Tests log(27, 3) with custom base — verifies logarithm with base 3 returns 3.0 (3^3=27), rounding to 4 decimals.
#[test]
fn test_log_base_custom() {
    let out = compile_and_run(
        r#"<?php
echo round(log(27, 3), 4);
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies clamp() returns an in-range integer value unchanged.
#[test]
fn test_clamp_int_inside_range() {
    let out = compile_and_run("<?php echo clamp(5, 0, 10);");
    assert_eq!(out, "5");
}

/// Verifies clamp() returns the upper integer bound when the value is too large.
#[test]
fn test_clamp_int_upper_bound() {
    let out = compile_and_run("<?php echo clamp(15, 0, 10);");
    assert_eq!(out, "10");
}

/// Verifies clamp() returns the lower integer bound when the value is too small.
#[test]
fn test_clamp_int_lower_bound() {
    let out = compile_and_run("<?php echo clamp(-5, 0, 10);");
    assert_eq!(out, "0");
}

/// Verifies clamp() preserves inclusive boundary equality.
#[test]
fn test_clamp_boundary_equality() {
    let out = compile_and_run("<?php echo clamp(0, 0, 10) . ':' . clamp(10, 0, 10);");
    assert_eq!(out, "0:10");
}

/// Verifies clamp() works with floating-point values and bounds.
#[test]
fn test_clamp_float() {
    let out = compile_and_run("<?php echo clamp(2.75, 1.5, 2.5);");
    assert_eq!(out, "2.5");
}

/// Verifies clamp() handles mixed integer and floating-point operands.
#[test]
fn test_clamp_mixed_int_float() {
    let out = compile_and_run("<?php echo clamp(2, 1.5, 3.5);");
    assert_eq!(out, "2");
}

/// Verifies clamp() uses lexicographic ordering for all-string operands.
#[test]
fn test_clamp_string_comparison() {
    let out = compile_and_run("<?php echo clamp('P', 'A', 'C') . ':' . clamp('P', 'X', 'Z');");
    assert_eq!(out, "C:X");
}

/// Verifies clamp() participates in case-insensitive lookup, namespace fallback, function_exists(), and first-class callable syntax.
#[test]
fn test_clamp_lookup_and_first_class_callable() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo function_exists("ClAmP") ? "1" : "0";
echo ":";
echo ClAmP(15, 0, 10);
echo ":";
$clamp = clamp(...);
echo $clamp(-1, 0, 10);
"#,
    );
    assert_eq!(out, "1:10:0");
}

/// Verifies clamp() throws a catchable ValueError when min is greater than max.
#[test]
fn test_clamp_invalid_bounds_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    clamp(5, 10, 0);
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e);
}
"#,
    );
    assert_eq!(out, "ValueError");
}

/// Verifies clamp() rejects NaN lower and upper bounds with catchable ValueError exceptions.
#[test]
fn test_clamp_nan_bounds_throw_value_error() {
    let out = compile_and_run(
        r#"<?php
try {
    clamp(5.0, NAN, 10.0);
    echo "bad-min";
} catch (ValueError $e) {
    echo get_class($e);
}
echo ":";
try {
    clamp(5.0, 0.0, NAN);
    echo "bad-max";
} catch (ValueError $e) {
    echo get_class($e);
}
"#,
    );
    assert_eq!(out, "ValueError:ValueError");
}

/// Regression for #369: constant `int + int` that overflows is folded to float
/// by the optimizer. No checked helper is needed.
#[test]
fn test_int_overflow_constant_folds_to_float() {
    let out = compile_and_run("<?php echo PHP_INT_MAX + 1;");
    assert_eq!(out, "9.2233720368548E+18");
}

/// Regression for #369: non-constant int arithmetic that does NOT overflow
/// stays as int (the checked helper returns an int-tagged Mixed box).
#[test]
fn test_int_no_overflow_stays_int() {
    let out = compile_and_run("<?php echo $argc + 41;");
    assert_eq!(out, "42");
}

/// Regression for #369: chained non-constant int arithmetic produces correct
/// results when intermediate values don't overflow.
#[test]
fn test_int_chained_arithmetic_no_overflow() {
    let out = compile_and_run("<?php echo $argc + 1 + 2 + 3;");
    assert_eq!(out, "7");
}

/// Regression for #369 Tier 2 Stage 0: a checked op with two constant operands
/// is folded at IR level by ConstFold. The result type narrows from Mixed to
/// Int when there is no overflow. This verifies the type-narrowing path works
/// end-to-end (acquire/release of the narrowed value, local store, echo).
#[test]
fn test_checked_op_constant_folds_no_overflow() {
    let out = compile_and_run(r#"<?php $x = 1 + 2; echo $x;"#);
    assert_eq!(out, "3");
}

/// Regression for #369 Tier 2 Stage 0: a checked op with two constant operands
/// that overflows is folded to a float constant by ConstFold. The result type
/// narrows from Mixed to Float.
#[test]
fn test_checked_op_constant_folds_overflow_to_float() {
    let out = compile_and_run(r#"<?php $x = 9223372036854775807 + 1; echo $x;"#);
    assert_eq!(out, "9.2233720368548E+18");
}

/// Regression for #369 Tier 2 Stage 0: a checked subtraction with two constant
/// operands that overflows is folded to a float constant.
#[test]
fn test_checked_sub_constant_folds_overflow_to_float() {
    let out = compile_and_run(r#"<?php $x = -9223372036854775808 - 1; echo $x;"#);
    assert_eq!(out, "-9.2233720368548E+18");
}

/// Regression for #369 Tier 2 Stage 0: a checked multiplication with two
/// constant operands that overflows is folded to a float constant.
#[test]
fn test_checked_mul_constant_folds_overflow_to_float() {
    let out = compile_and_run(r#"<?php $x = 9223372036854775807 * 2; echo $x;"#);
    assert_eq!(out, "1.844674407371E+19");
}

/// Regression for #369 Tier 2 Stage 0: a checked op folded to a constant and
/// stored to a local, then used in a subsequent checked op that also folds.
/// Verifies chained constant propagation through local slots with type
/// narrowing.
#[test]
fn test_checked_op_chained_constant_folds() {
    let out = compile_and_run(r#"<?php $a = 100 + 200; $b = $a + 300; echo $b;"#);
    assert_eq!(out, "600");
}

/// Regression for #369 Tier 2 Stage 0: a checked op with two constant operands
/// that does NOT overflow folds to an Int constant, and the result is used in a
/// Mixed-typed context (var_dump) to verify the type narrowing is safe.
#[test]
fn test_checked_op_constant_folds_no_overflow_var_dump() {
    let out = compile_and_run(r#"<?php $x = 42 + 8; var_dump($x);"#);
    assert_eq!(out, "int(50)\n");
}

// --- abs() overflow promotion (PHP_INT_MIN) ---

/// Verifies `abs(PHP_INT_MIN)` promotes to float like PHP instead of wrapping to a negative int.
///
/// `PHP_INT_MIN` is the one input with no `int` absolute value; PHP returns
/// `float(9.2233720368547758E+18)`. The `intdiv(..., $argc)` keeps the operand an `int`-typed
/// runtime value so the folders cannot evaluate `abs()` at compile time.
#[test]
fn test_abs_int_min_promotes_to_float() {
    let out = compile_and_run(
        r#"<?php
$min = intdiv(PHP_INT_MIN, $argc);
var_dump(gettype(abs($min)));
var_dump(abs($min) > 9.223372036854e18);
var_dump(abs(PHP_INT_MIN) > 9.223372036854e18);
var_dump(gettype(abs(PHP_INT_MIN)));
"#,
    );
    assert_eq!(
        out,
        "string(6) \"double\"\nbool(true)\nbool(true)\nstring(6) \"double\"\n"
    );
}

/// Verifies `abs()` on a boxed Mixed `PHP_INT_MIN` payload promotes to float as well.
///
/// Runtime int arithmetic that can overflow is typed `Mixed`, so `abs()` on it goes through
/// `__rt_abs_mixed` rather than the inline integer lowering.
#[test]
fn test_abs_mixed_int_min_promotes_to_float() {
    let out = compile_and_run(
        r#"<?php
$min = PHP_INT_MIN * $argc;
var_dump(gettype(abs($min)));
var_dump(abs($min) > 9.223372036854e18);
"#,
    );
    assert_eq!(out, "string(6) \"double\"\nbool(true)\n");
}

/// Verifies every non-overflowing `abs()` input keeps PHP's exact value and type.
#[test]
fn test_abs_keeps_int_and_float_results() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump(abs(-42));
var_dump(abs(42));
var_dump(abs(0));
var_dump(abs(-3.5));
var_dump(abs(intdiv(-7, $n)));
var_dump(gettype(abs(intdiv(-7, $n))));
var_dump(abs(PHP_INT_MAX * $n));
echo abs(-42), '|', abs(intdiv(-7, $n)), '|', abs(-2.5);
"#,
    );
    assert_eq!(
        out,
        "int(42)\nint(42)\nint(0)\nfloat(3.5)\nint(7)\nstring(7) \"integer\"\n\
         int(9223372036854775807)\n42|7|2.5"
    );
}

/// Verifies `fmod()` keeps the sign of the dividend, including the negative zero PHP
/// prints as `-0`. Computing it as `x - trunc(x / y) * y` silently produces `+0.0`
/// instead, which is why both targets call libc `fmod`. `* $argc` keeps the operands on
/// the runtime path.
#[test]
fn test_fmod_preserves_negative_zero_like_php() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
echo fmod(-7.5 * $n, 2.5), "|", fmod(7.5 * $n, 2.5), "|", fmod(-5.5 * $n, 2.0), "|", fmod(5.5 * $n, 2.0), "|", fmod(1.0 * $n, 0.0), "|";
var_dump(fmod(-7.5 * $n, 2.5));
"#,
    );
    assert_eq!(out, "-0|0|-1.5|1.5|NAN|float(-0)\n");
}

/// Verifies PHP's single-array `min()` / `max()` form over int, float, and bool arrays,
/// through both a literal and a variable. Expected output matches `php -r` on 8.4.
#[test]
fn test_min_max_single_array_argument() {
    let out = compile_and_run(
        r#"<?php
var_dump(min([1, 2, 3]), max([1, 2, 3]));
$a = [3, 1, 2];
var_dump(min($a), max($a));
$b = [3.5, 1.25, 2.0];
var_dump(min($b), max($b));
var_dump(min([5]), max([5]));
var_dump(min([true, false]), max([true, false]));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(3)\nint(1)\nint(3)\nfloat(1.25)\nfloat(3.5)\nint(5)\nint(5)\n\
         bool(false)\nbool(true)\n"
    );
}

/// Verifies the variadic `min()` / `max()` form still works next to the single-array
/// form, including the runtime-unknown operands that survive constant folding.
#[test]
fn test_min_max_variadic_form_still_works() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
var_dump(min(1, 2), max(1, 2, 3), min(1.5, 2));
var_dump(min($n, 5), max($n, 5));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(3)\nfloat(1.5)\nint(1)\nint(5)\n"
    );
}

/// Verifies an empty array raises PHP's catchable `ValueError` with php-src's exact
/// message, for both `min()` and `max()` and for a literal and a variable. The third case
/// discards the result: `min`/`max` are modeled as `MAY_THROW`, so dead-code elimination
/// must not drop the call and with it the diagnostic.
#[test]
fn test_min_max_empty_array_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
try { var_dump(min([])); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
$empty = [];
try { var_dump(max($empty)); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { min([]); } catch (ValueError $e) { echo "discarded: ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "ValueError: min(): Argument #1 ($value) must contain at least one element\n\
         ValueError: max(): Argument #1 ($value) must contain at least one element\n\
         discarded: min(): Argument #1 ($value) must contain at least one element\n"
    );
}

/// Verifies the single-array `min()` / `max()` form over an indexed `array<string>`,
/// including PHP's numeric-string promotion (`"10" < "9"` is false because both are
/// numeric, while `"10" < "9a"` falls back to a byte comparison). Expected output is
/// verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_min_max_single_array_of_strings() {
    let out = compile_and_run(
        r#"<?php
var_dump(min(["a", "c", "b"]), max(["a", "c", "b"]));
$s = ["pear", "apple", "fig"];
var_dump(min($s), max($s));
var_dump(min(["10", "9"]), max(["10", "9"]));
var_dump(min(["10", "9a"]), max(["10", "9a"]));
var_dump(min(["1e2", "99"]), max(["1e2", "99"]));
"#,
    );
    assert_eq!(
        out,
        "string(1) \"a\"\nstring(1) \"c\"\n\
         string(5) \"apple\"\nstring(4) \"pear\"\n\
         string(1) \"9\"\nstring(2) \"10\"\n\
         string(2) \"10\"\nstring(2) \"9a\"\n\
         string(2) \"99\"\nstring(3) \"1e2\"\n"
    );
}

/// Verifies the single-array form over a heterogeneous (boxed `Mixed`) indexed array.
/// Every case pins one rule of PHP 8's comparison table: a bool operand coerces both
/// sides, `null` versus a number coerces to bool, a number versus a non-numeric string
/// compares as strings, and equal elements keep the *earlier* one. Expected output is
/// verbatim `LC_ALL=C php` 8.4 output for the same program.
#[test]
fn test_min_max_single_array_mixed_elements() {
    let out = compile_and_run(
        r#"<?php
var_dump(min([1, 2.5]), max([1, 2.5]));
var_dump(min([1, "1"]), max([1, "1"]));
var_dump(min([0, "a"]), max([0, "a"]));
var_dump(min([true, 2]), max([true, 2]));
var_dump(min([null, -1]), max([null, -1]));
var_dump(min([null, 0]), max([null, 0]));
var_dump(min([-1, "-2"]), max([-1, "-2"]));
var_dump(min([1, 2, "3"]), max([1, 2, "3"]));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nfloat(2.5)\n\
         int(1)\nint(1)\n\
         int(0)\nstring(1) \"a\"\n\
         bool(true)\nbool(true)\n\
         NULL\nint(-1)\n\
         NULL\nNULL\n\
         string(2) \"-2\"\nint(-1)\n\
         int(1)\nstring(1) \"3\"\n"
    );
}

/// Verifies the single-array form over hash-backed associative arrays, whose values may
/// be of any type. Expected output is verbatim `LC_ALL=C php` 8.4 output for the same
/// program.
#[test]
fn test_min_max_single_associative_array() {
    let out = compile_and_run(
        r#"<?php
var_dump(min(["a" => 3, "b" => 1, "c" => 2]), max(["a" => 3, "b" => 1, "c" => 2]));
$h = ["x" => "pear", "y" => "apple"];
var_dump(min($h), max($h));
var_dump(min(["a" => 1.5, "b" => 2.5]), max(["a" => 1.5, "b" => 2.5]));
var_dump(min(["a" => null, "b" => 1, "c" => "z"]), max(["a" => null, "b" => 1, "c" => "z"]));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(3)\n\
         string(5) \"apple\"\nstring(4) \"pear\"\n\
         float(1.5)\nfloat(2.5)\n\
         NULL\nstring(1) \"z\"\n"
    );
}

/// Verifies the container reductions on arrays built at run time from `$argc`, so the
/// elements survive constant folding and the loop really walks runtime storage. The
/// string result is copied out of the container, so it stays valid after the argument
/// temporary is released. Expected output is verbatim `LC_ALL=C php` 8.4 output.
#[test]
fn test_min_max_single_array_built_at_runtime() {
    let out = compile_and_run(
        r#"<?php
$n = $argc;
$a = [];
for ($i = 0; $i < 6; $i++) { $a[] = (($i * 5) % 7) + $n; }
var_dump(min($a), max($a));
$s = [];
for ($i = 0; $i < 4; $i++) { $s[] = "k" . ((($i * 3) % 4) + $n); }
var_dump(min($s), max($s));
$m = [];
for ($i = 0; $i < 4; $i++) { $m[] = ($i % 2 === 0) ? ($i + 0.5) : ("v$i"); }
var_dump(min($m), max($m));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nint(7)\n\
         string(2) \"k1\"\nstring(2) \"k4\"\n\
         float(0.5)\nstring(2) \"v3\"\n"
    );
}

/// Verifies `dechex()`, `decbin()`, and `decoct()` render an integer in their base.
/// Expectations are verbatim `LC_ALL=C php` 8.4 output for the same expressions.
#[test]
fn test_dec_to_base_positive_values() {
    let out = compile_and_run(
        r#"<?php
echo dechex(255), "|", dechex(26), "|", dechex(0), "|",
     decbin(26), "|", decbin(0), "|",
     decoct(64), "|", decoct(8), "|", decoct(0);
"#,
    );
    assert_eq!(out, "ff|1a|0|11010|0|100|10|0");
}

/// Verifies the base renderers treat their input as UNSIGNED, which is what makes
/// `dechex(-1)` print `ffffffffffffffff` in reference PHP rather than a signed value.
#[test]
fn test_dec_to_base_negative_is_unsigned() {
    let out = compile_and_run(
        r#"<?php
echo dechex(-1), "\n", decoct(-1), "\n", decbin(-1), "\n", dechex(PHP_INT_MAX), "\n";
"#,
    );
    assert_eq!(
        out,
        "ffffffffffffffff\n\
1777777777777777777777\n\
1111111111111111111111111111111111111111111111111111111111111111\n\
7fffffffffffffff\n"
    );
}

/// Verifies the base renderers resolve case-insensitively, through a namespace-qualified
/// call, and by named argument.
#[test]
fn test_dec_to_base_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php echo DECHEX(255), "|", \decbin(5), "|", decoct(num: 8);"#,
    );
    assert_eq!(out, "ff|101|10");
}

/// Verifies the base renderers work on runtime-unknown values inside a loop, so the shared
/// concat-scratch reservation is exercised repeatedly rather than folded away.
#[test]
fn test_dec_to_base_runtime_values_in_loop() {
    let out = compile_and_run(
        r#"<?php
$acc = "";
for ($i = 0; $i < 5; $i++) {
    $acc .= dechex($i * 1000 + $argc) . ":" . decoct($i + $argc) . ";";
}
echo $acc;
"#,
    );
    assert_eq!(out, "1:1;3e9:2;7d1:3;bb9:4;fa1:5;");
}

/// Verifies `hexdec()`, `bindec()`, and `octdec()` parse digits of their base.
/// Expectations are verbatim `LC_ALL=C php` 8.4 output for the same expressions.
#[test]
fn test_base_to_number_basic_values() {
    let out = compile_and_run(
        r#"<?php
var_dump(hexdec("ff"), hexdec("FF"), hexdec("a0"), hexdec(""),
         bindec("110"), bindec(""), octdec("77"), octdec(""));
"#,
    );
    assert_eq!(
        out,
        "int(255)\nint(255)\nint(160)\nint(0)\nint(6)\nint(0)\nint(63)\nint(0)\n"
    );
}

/// Verifies unary negation accepts a runtime `int|float` result and preserves PHP numeric value.
#[test]
fn test_negate_base_conversion_numeric_union() {
    let out = compile_and_run(r#"<?php echo -octdec("17");"#);
    assert_eq!(out, "-15");
}

/// Verifies the base parsers ignore characters that are not digits of the requested base,
/// which is what makes `hexdec("a0z")` `160` and `bindec("12")` `1` in reference PHP.
/// (PHP additionally raises an `E_DEPRECATED` notice here; elephc emits no deprecation
/// diagnostics, so only the value is compared.)
#[test]
fn test_base_to_number_ignores_invalid_characters() {
    let out = compile_and_run(
        r#"<?php var_dump(hexdec("a0z"), bindec("12"), octdec("98"), hexdec("0xff"));"#,
    );
    assert_eq!(out, "int(160)\nint(1)\nint(0)\nint(255)\n");
}

/// Verifies the base parsers widen to `float` exactly where reference PHP does — at
/// `PHP_INT_MAX`, not at the unsigned 64-bit boundary.
#[test]
fn test_base_to_number_widens_to_float_past_int_max() {
    let out = compile_and_run(
        r#"<?php
var_dump(hexdec("7fffffffffffffff"), hexdec("8000000000000000"),
         hexdec("ffffffffffffffff"), octdec("1777777777777777777777"));
"#,
    );
    assert_eq!(
        out,
        "int(9223372036854775807)\n\
float(9.223372036854776E+18)\n\
float(1.8446744073709552E+19)\n\
float(1.8446744073709552E+19)\n"
    );
}

/// Verifies the base parsers resolve case-insensitively, through a namespace-qualified
/// call, and by named argument.
#[test]
fn test_base_to_number_case_insensitive_namespaced_and_named_args() {
    let out = compile_and_run(
        r#"<?php var_dump(HEXDEC("ff"), \bindec("101"), octdec(octal_string: "17"));"#,
    );
    assert_eq!(out, "int(255)\nint(5)\nint(15)\n");
}

/// Verifies `dechex()`/`hexdec()` round-trip a runtime-unknown value, so neither side is
/// folded away by the optimizer.
#[test]
fn test_dechex_hexdec_roundtrip_runtime_value() {
    let out = compile_and_run(
        r#"<?php
$n = 48879 + $argc;
var_dump(hexdec(dechex($n)) === $n, bindec(decbin($n)) === $n, octdec(decoct($n)) === $n);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies the new base-conversion builtins keep their PHP types inside an array literal.
///
/// Array-literal element typing goes through the checker's syntactic inference table, whose
/// fallback is `int`. Before these builtins were listed there, `[dechex($n)]` typed its
/// element `int` and read the string result registers as an integer, so `["a"]` printed
/// `[0]`. Expectations are verbatim `LC_ALL=C php` 8.4 output.
#[test]
fn test_base_builtins_keep_their_types_inside_array_literals() {
    let out = compile_and_run(
        r#"<?php
$i = 10 + $argc - 1;
var_dump([dechex($i), decoct($i), decbin($i)]);
var_dump([hexdec("ff"), octdec("17"), hexdec("ffffffffffffffff")]);
echo join("/", [dechex($i), decoct($i)]), "\n";
"#,
    );
    assert_eq!(
        out,
        "array(3) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(2) \"12\"\n  [2]=>\n  string(4) \"1010\"\n}\n\
array(3) {\n  [0]=>\n  int(255)\n  [1]=>\n  int(15)\n  [2]=>\n  float(1.8446744073709552E+19)\n}\n\
a/12\n"
    );
}

/// Verifies `base_convert()` re-renders numerals across bases, ignoring characters that are
/// not digits of the source base and treating letter digits case-insensitively.
#[test]
fn test_base_convert() {
    let out = compile_and_run(
        r#"<?php
echo base_convert("ff", 16, 10), "|", base_convert("255", 10, 16), "|",
     base_convert("a37334", 16, 2), "|", base_convert("FF", 16, 10), "|",
     base_convert("zz", 36, 10), "|", base_convert("", 10, 16), "|",
     base_convert("0", 10, 2), "|", base_convert("123abcz", 16, 7);
"#,
    );
    assert_eq!(out, "255|ff|101000110111001100110100|255|1295|0|0|13104021");
}

/// Verifies `base_convert()` reproduces php-src's lossy float render for values past
/// `PHP_INT_MAX`, where the parse widens to `double` and the digits come out rounded.
#[test]
fn test_base_convert_widens_past_int_max() {
    let out = compile_and_run(
        r#"<?php
echo base_convert("7fffffffffffffff", 16, 10), "\n",
     base_convert("ffffffffffffffff", 16, 10), "\n",
     base_convert("18446744073709551615", 10, 16), "\n",
     base_convert("ffffffffffffffffffffffffffffff", 16, 10), "\n",
     base_convert("ffffffffffffffffffffffffffffff", 16, 36);
"#,
    );
    assert_eq!(
        out,
        "9223372036854775807\n18446744073709552046\n10000000000000000\n\
         1329227995784916008062602462446642046\n24q5bylddqn4ggg0sw8k8s0s"
    );
}

/// Verifies `base_convert()` stops at php-src's 64-digit float buffer bound.
#[test]
fn test_base_convert_float_digit_cap() {
    let out = compile_and_run(
        r#"<?php $r = base_convert(str_repeat("z", 40), 36, 2); echo strlen($r), "|", $r;"#,
    );
    assert_eq!(
        out,
        "64|0000000000000000000000000000000000000000000000000000000000000000"
    );
}

/// Verifies `base_convert()` resolves through case-insensitive, namespaced, and
/// named-argument call forms.
#[test]
fn test_base_convert_case_insensitive_and_named() {
    let out = compile_and_run(
        r#"<?php
echo Base_Convert("ff", 16, 10), "|", \base_convert("ff", 16, 8), "|",
     base_convert(num: "ff", from_base: 16, to_base: 10);
"#,
    );
    assert_eq!(out, "255|377|255");
}

/// Verifies `base_convert()` raises php-src's `ValueError` for either base outside 2..36.
#[test]
fn test_base_convert_base_out_of_range_throws() {
    let out = compile_and_run(
        r#"<?php
try { base_convert("f", 1, 10); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { base_convert("f", 37, 10); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { base_convert("f", 16, 1); } catch (\ValueError $e) { echo $e->getMessage(), "\n"; }
try { base_convert("f", 16, 37); } catch (\ValueError $e) { echo $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "base_convert(): Argument #2 ($from_base) must be between 2 and 36 (inclusive)\n\
         base_convert(): Argument #2 ($from_base) must be between 2 and 36 (inclusive)\n\
         base_convert(): Argument #3 ($to_base) must be between 2 and 36 (inclusive)\n\
         base_convert(): Argument #3 ($to_base) must be between 2 and 36 (inclusive)"
    );
}
