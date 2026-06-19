//! Purpose:
//! Regression tests for `json_encode` float formatting at PHP's
//! `serialize_precision = -1` (shortest decimal that round-trips), driven by
//! the `__rt_json_ftoa` runtime helper.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Each expected string is the exact output of `php -r 'echo json_encode(x);'`
//!   on PHP 8.x: lowercase `e`, a `d.d` mantissa in exponential form, an
//!   exponent with no leading zeros, and NO trailing `.0` for integer-valued
//!   floats. These would all fail under the previous precision-14 `__rt_ftoa`
//!   delegation (e.g. `0.33333333333333`, `1E+17`, `1E-06`).

use super::*;

/// Verifies a repeating fraction keeps 16 significant digits (1/3), not the
/// 14 digits the old `%.14G` formatter produced.
#[test]
fn test_json_encode_one_third_shortest() {
    let out = compile_and_run("<?php echo json_encode(1.0/3.0);");
    assert_eq!(out, "0.3333333333333333");
}

/// Verifies the canonical 0.1+0.2 case renders all 17 significant digits.
#[test]
fn test_json_encode_point_one_plus_point_two() {
    let out = compile_and_run("<?php echo json_encode(0.1 + 0.2);");
    assert_eq!(out, "0.30000000000000004");
}

/// Verifies a large magnitude uses exponential form with lowercase `e`, a
/// `1.0` mantissa, and a `+` exponent with no leading zero.
#[test]
fn test_json_encode_large_exponential() {
    let out = compile_and_run("<?php echo json_encode(1.0e17);");
    assert_eq!(out, "1.0e+17");
}

/// Verifies a small magnitude uses exponential form with a single-digit
/// exponent and no leading zero (`1.0e-6`, not `1.0E-06`).
#[test]
fn test_json_encode_small_exponential() {
    let out = compile_and_run("<?php echo json_encode(0.000001);");
    assert_eq!(out, "1.0e-6");
}

/// Verifies the decimal/exponential boundary on the small side: 0.0001 stays
/// decimal (decpt == -3) while 0.00001 switches to exponential (decpt < -3).
#[test]
fn test_json_encode_small_decimal_boundary() {
    let out = compile_and_run(
        "<?php echo json_encode(0.0001), '|', json_encode(0.00001);",
    );
    assert_eq!(out, "0.0001|1.0e-5");
}

/// Verifies the decimal/exponential boundary on the large side: 1e16 stays
/// decimal (decpt == 17) while 1e17 switches to exponential (decpt > 17).
#[test]
fn test_json_encode_large_decimal_boundary() {
    let out = compile_and_run(
        "<?php echo json_encode(1.0e16), '|', json_encode(1.0e17);",
    );
    assert_eq!(out, "10000000000000000|1.0e+17");
}

/// Verifies integer-valued floats drop the fractional part in JSON (PHP emits
/// `100`/`1`, unlike `var_export`'s `100.0`/`1.0`).
#[test]
fn test_json_encode_integer_valued_floats() {
    let out = compile_and_run(
        "<?php echo json_encode(100.0), '|', json_encode(1.0), '|', json_encode(2.0);",
    );
    assert_eq!(out, "100|1|2");
}

/// Verifies signed zero round-trips to `-0` and positive zero to `0`.
#[test]
fn test_json_encode_signed_zero() {
    let out = compile_and_run(
        "<?php echo json_encode(0.0), '|', json_encode(-0.0);",
    );
    assert_eq!(out, "0|-0");
}

/// Verifies a negative fractional value keeps its sign and shortest digits.
#[test]
fn test_json_encode_negative_fraction() {
    let out = compile_and_run("<?php echo json_encode(-2.5), '|', json_encode(-123.456);");
    assert_eq!(out, "-2.5|-123.456");
}

/// Verifies a three-digit exponent magnitude (1e300, 1e-300) is emitted with
/// no leading zeros.
#[test]
fn test_json_encode_three_digit_exponent() {
    let out = compile_and_run(
        "<?php echo json_encode(1.0e300), '|', json_encode(1.0e-300);",
    );
    assert_eq!(out, "1.0e+300|1.0e-300");
}

/// Verifies floats nested inside an array each format with shortest precision.
#[test]
fn test_json_encode_floats_in_array() {
    let out = compile_and_run("<?php echo json_encode([1.0/3.0, 100.0, 1.0e17, 1.5]);");
    assert_eq!(out, "[0.3333333333333333,100,1.0e+17,1.5]");
}

/// Verifies floats nested as associative-array values keep shortest precision.
#[test]
fn test_json_encode_floats_in_assoc() {
    let out = compile_and_run(
        r#"<?php echo json_encode(["pi" => 3.141592653589793, "e" => 0.000001]);"#,
    );
    assert_eq!(out, r#"{"pi":3.141592653589793,"e":1.0e-6}"#);
}

/// Verifies JSON_PRESERVE_ZERO_FRACTION re-adds `.0` to an integer-valued
/// float in decimal form while leaving exponential output untouched.
#[test]
fn test_json_encode_preserve_zero_fraction() {
    let out = compile_and_run(
        "<?php echo json_encode(100.0, JSON_PRESERVE_ZERO_FRACTION), '|', \
         json_encode(1.0e17, JSON_PRESERVE_ZERO_FRACTION);",
    );
    assert_eq!(out, "100.0|1.0e+17");
}

/// Verifies a value needing exactly 16 significant digits in non-exponential
/// form (decpt within range) renders without trailing `.0`.
#[test]
fn test_json_encode_sixteen_digit_integer_float() {
    let out = compile_and_run("<?php echo json_encode(1234567890123456.0);");
    assert_eq!(out, "1234567890123456");
}
