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
