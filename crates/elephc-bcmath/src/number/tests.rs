//! Purpose:
//! Pins Number's automatic decimal scales and method-specific failures.
//!
//! Called from:
//! - `cargo test -p elephc-bcmath number`.
//!
//! Key details:
//! - Expected results are cross-checked with PHP 8.5's BCMath implementation.

use super::*;

/// Preserves full constructor scale while normalizing signs and leading zeroes.
#[test]
fn number_constructor_preserves_scale() {
    for (input, expected) in [("001.200", "1.200"), ("-0.000", "0.000"), ("+", "0"), (".50", "0.50")] {
        assert_eq!(number_create(input).unwrap(), expected);
    }
}

/// Uses full operand scale for addition/modulo and their sum for multiplication.
#[test]
fn number_binary_automatic_scales() {
    for (method, left, right, expected) in [
        ("add", "1.200", "2.01", "3.210"),
        ("sub", "1.200", "2", "-0.800"),
        ("mul", "1.200", "2.00", "2.40000"),
        ("mod", "1.200", "0.50", "0.200"),
        ("pow", "1.200", "2", "1.440000"),
        ("pow", "1.200", "0", "1"),
    ] {
        assert_eq!(number_binary(method, left, right, None).unwrap(), expected);
    }
}

/// Removes expansion zeroes without truncating the receiver's full scale.
#[test]
fn number_expansion_preserves_minimum_scale() {
    for (left, right, expected) in [("1.200", "2", "0.600"), ("1", "3", "0.3333333333"), ("1.00", "8", "0.125")] {
        assert_eq!(number_binary("div", left, right, None).unwrap(), expected);
    }
    assert_eq!(number_sqrt("4.00", None).unwrap(), "2.00");
    assert_eq!(number_sqrt("2", None).unwrap(), "1.4142135623");
    assert_eq!(number_binary("pow", "2.00", "-3", None).unwrap(), "0.125");
    assert_eq!(number_binary("div", "1.200", "2", Some(5)).unwrap(), "0.60000");
}

/// Keeps quotient integral, modular power independent of scale, and comparison exact.
#[test]
fn number_other_method_shapes() {
    assert_eq!(number_divmod("1.200", ".20", None).unwrap(), ("6".into(), "0.000".into()));
    assert_eq!(number_powmod("4", "13", "497", None).unwrap(), "445");
    assert_eq!(number_compare("1.200", "1.201", None).unwrap(), -1);
    assert_eq!(number_compare("1.200", "1.201", Some(2)).unwrap(), 0);
    assert_eq!(number_rounding("round", "1.200", 5, 1).unwrap(), "1.20000");
}

/// Retains method and argument names on malformed inputs and scale errors.
#[test]
fn number_method_error_context() {
    assert_eq!(number_create("1e2").unwrap_err().php_message(), "BcMath\\Number::__construct(): Argument #1 ($num) is not well-formed");
    assert_eq!(number_binary("add", "1", "2", Some(-1)).unwrap_err().php_message(), "BcMath\\Number::add(): Argument #2 ($scale) must be between 0 and 2147483647");
    assert_eq!(number_sqrt("-1", None).unwrap_err().php_message(), "Base number must be greater than or equal to 0");
}

/// Accepts PHP's machine-integer exponent range without expanding insignificant zeroes.
#[test]
fn number_power_uses_machine_integer_exponents() {
    assert_eq!(number_binary("pow", "1.00", "2147483648", Some(2)).unwrap(), "1.00");
    assert_eq!(number_binary("pow", "-1", "9223372036854775807", None).unwrap(), "-1");
    assert_eq!(number_binary("pow", "1", "-9223372036854775808", None).unwrap(), "1");
    assert_eq!(number_binary("pow", "1.00", "9223372036854775807", None).unwrap_err().php_message(), "scale of the result is too large");
}

/// Huge negative precision rounds toward zero without constructing billions of zero digits.
#[test]
fn number_rounding_accepts_machine_integer_negative_precision() {
    assert_eq!(number_rounding("round", "2.00", i64::MIN, 1).unwrap(), "0");
    assert_eq!(number_rounding("round", "-12.00", -2147483649, 7).unwrap(), "0");
    assert_eq!(number_rounding("round", "1", 2147483648, 1).unwrap_err().php_message(),
        "BcMath\\Number::round(): Argument #1 ($precision) must be between -9223372036854775808 and 2147483647");
}

/// PHP checks a negative modular exponent before a well-formed fractional modulus.
#[test]
fn number_powmod_validation_order() {
    assert_eq!(number_powmod("4", "-1", "1.5", None).unwrap_err().php_message(),
        "BcMath\\Number::powmod(): Argument #1 ($exponent) must be greater than or equal to 0");
    assert_eq!(number_powmod("4", "-1", "invalid", None).unwrap_err().php_message(),
        "BcMath\\Number::powmod(): Argument #2 ($modulus) is not well-formed");
}

/// Binary method ABI errors name the public operand, independently of the hidden receiver.
#[test]
fn number_abi_invalid_utf8_names_public_operand() {
    let invalid = [0xff];
    let mut result = std::ptr::null_mut();
    let mut length = 0;
    let status = unsafe {
        crate::elephc_bcmath_number_pow(b"1".as_ptr(), 1, invalid.as_ptr(), 1,
            0, 1, &mut result, &mut length)
    };
    assert_eq!(status, crate::BCMATH_ERR_MALFORMED);
    assert!(result.is_null());
    assert_eq!(length, 0);
    let mut message = std::ptr::null();
    let mut message_len = 0;
    unsafe { crate::elephc_bcmath_last_error(&mut message, &mut message_len); }
    let message = unsafe { std::slice::from_raw_parts(message, message_len) };
    assert_eq!(message, b"BcMath\\Number::pow(): Argument #1 ($exponent) is not well-formed");
}
