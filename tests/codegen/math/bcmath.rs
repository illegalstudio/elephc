//! Purpose:
//! End-to-end integration tests for the PHP BCMath builtin surface.
//!
//! Called from:
//! - `cargo test --test codegen_tests_data` through the math test group.
//!
//! Key details:
//! - Expected strings follow PHP BCMath truncation and fixed-scale formatting.
//! - Runtime values keep calls on the EIR and native bridge path.

use crate::support::*;

/// Verifies `bcadd()` crosses the native bridge and preserves explicit scale.
#[test]
fn test_bcadd_explicit_scale() {
    let out = compile_and_run(
        r#"<?php
$left = "1.234";
$right = "5";
echo bcadd($left, $right, 4);
"#,
    );
    assert_eq!(out, "6.2340");
}

/// Verifies digitless operands become zero while surrounding whitespace remains malformed.
#[test]
fn test_bcmath_php_numeric_string_grammar() {
    let out = compile_and_run(
        r#"<?php
foreach (['', '+', '-', '.', '+.', '-.'] as $zero) {
    echo bcadd($zero, '2', 2), '|';
}
foreach ([' 0', '0 ', "\t0"] as $bad) {
    try {
        bcadd($bad, '2', 2);
    } catch (\ValueError $e) {
        echo get_class($e), '|';
    }
}
"#,
    );
    assert_eq!(
        out,
        "2.00|2.00|2.00|2.00|2.00|2.00|ValueError|ValueError|ValueError|"
    );
}

/// Verifies the scaled arithmetic, comparison, power, and root operations together.
#[test]
fn test_bcmath_arithmetic_surface() {
    let out = compile_and_run(
        r#"<?php
echo bcsub('5', '1.25', 2), '|';
echo bcmul('2.5', '4', 3), '|';
echo bcdiv('105', '6.55957', 3), '|';
echo bcmod('5', '3', 0), '|';
echo bcpow('2', '-3', 4), '|';
echo bcpowmod('4', '13', '497', 0), '|';
echo bcsqrt('2', 3), '|';
echo bccomp('1.00', '1.001', 2);
"#,
    );
    assert_eq!(out, "3.75|10.000|16.007|2|0.1250|445|1.414|0");
}

/// Verifies integer boundaries and all BCMath rounding direction families used by PHP 8.4.
#[test]
fn test_bcmath_integer_boundaries_and_rounding() {
    let out = compile_and_run(
        r#"<?php
echo bcceil('-1.2'), '|', bcfloor('-1.2'), '|';
echo bcround('9.5', 0, 1), '|', bcround('9.5', 0, 2), '|';
echo bcround('12.345', 2, 3), '|', bcround('12.355', 2, 4);
"#,
    );
    assert_eq!(out, "-1|-2|10|9|12.34|12.35");
}

/// Verifies directional rounding modes 5 through 8 use PHP's integer mapping.
#[test]
fn test_bcround_directional_modes_match_php() {
    let out = compile_and_run(
        r#"<?php
foreach ([5, 6, 7, 8] as $mode) {
    echo bcround('9.5', 0, $mode), ':', bcround('-9.5', 0, $mode), '|';
}
"#,
    );
    assert_eq!(out, "10:-9|9:-10|9:-9|10:-10|");
}

/// Verifies `bcscale()` setter/getter state and omitted-scale arithmetic in one process.
#[test]
fn test_bcscale_then_omitted_scale() {
    let out = compile_and_run(
        r#"<?php
echo bcscale(3), '|', bcscale(), '|', bcdiv('105', '6.55957');
"#,
    );
    assert_eq!(out, "0|3|16.007");
}

/// Verifies AOT and Magician calls observe the same process-wide BCMath scale.
#[test]
fn test_bcscale_is_shared_with_eval() {
    let out = compile_and_run(
        r#"<?php
bcscale(4);
eval('echo bcmul("1", "1");');
"#,
    );
    assert_eq!(out, "1.0000");
}

/// Verifies a runtime nullable scale chooses process scale while explicit zero remains distinct.
#[test]
fn test_bcmath_dynamic_nullable_scale() {
    let out = compile_and_run(
        r#"<?php
bcscale(2);
$scale = $argc > 1 ? 0 : null;
echo bcadd('1.234', '5', $scale), '|', bcadd('1.234', '5', 0);
"#,
    );
    assert_eq!(out, "6.23|6");
}

/// Verifies BCMath names are case-insensitive and named arguments use php-src keys.
#[test]
fn test_bcmath_case_insensitive_and_named_args() {
    let out = compile_and_run(
        r#"<?php echo BCADD(num1: '1', num2: '2', scale: 0), '|', \bCsUb(num1: '5', num2: '2', scale: 0);"#,
    );
    assert_eq!(out, "3|3");
}

/// Verifies `bcdivmod()` returns quotient and remainder with PHP's dividend sign rule.
#[test]
fn test_bcdivmod_signs() {
    let out = compile_and_run(
        r#"<?php
[$q, $r] = bcdivmod('-5', '3');
echo $q, '|', $r;
"#,
    );
    assert_eq!(out, "-1|-2");
}

/// Verifies malformed decimals and zero divisors become catchable PHP throwable classes.
#[test]
fn test_bcmath_failures_are_catchable() {
    let out = compile_and_run(
        r#"<?php
$bad = '1e2';
try { bcadd($bad, '1'); } catch (\ValueError $e) { echo get_class($e), '|', $e->getMessage(), "\n"; }
try { bcdiv('1', '0'); } catch (\DivisionByZeroError $e) { echo get_class($e), '|', $e->getMessage(), "\n"; }
try { bcround('1', 0, 9); } catch (\ValueError $e) { echo get_class($e), '|', $e->getMessage(); }
"#,
    );
    assert_eq!(
        out,
        "ValueError|bcadd(): Argument #1 ($num1) is not well-formed\nDivisionByZeroError|Division by zero\nValueError|bcround(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)"
    );
}

/// Verifies the BCMath procedural surface participates in builtin discovery.
#[test]
fn test_function_exists_bcadd() {
    let out = compile_and_run(r#"<?php echo function_exists('bcadd') ? 'yes' : 'no';"#);
    assert_eq!(out, "yes");
}
