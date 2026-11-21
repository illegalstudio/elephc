//! Purpose:
//! Tests numeric boundary behavior and exact-versus-PHP array-key classification.
//!
//! Called from:
//! - Cargo's elephc-semantics scalar integration-test binary.
//!
//! Key details:
//! - Expectations include exact integer limits and binary string keys.

use elephc_semantics::{SemanticProfile, StrictFeature};
use elephc_semantics::array_keys::{classify_string_key, php_integer_key, StringKey};
use elephc_semantics::numeric::{checked_float_to_int, checked_integer, checked_string_to_int, IntegerOp, NumericFailure};

/// Numeric spellings retain their bytes and kind under the exact-key feature alone.
#[test]
fn strict_scalar_exact_keys_preserve_every_string() {
    let exact = SemanticProfile::PERMISSIVE.with(StrictFeature::ArrayKeys, true);
    for bytes in [b"1".as_slice(), b"0", b"01", b"-0", b"-1", b"a", b"", b"1\0", b" 1", b"9223372036854775808"] {
        assert_eq!(classify_string_key(exact, bytes), StringKey::String(bytes));
    }
}

/// PHP-compatible normalization accepts only canonical in-range integer spellings.
#[test]
fn strict_scalar_php_key_controls_keep_canonical_limits() {
    for (bytes, expected) in [(b"0".as_slice(), 0), (b"1", 1), (b"-1", -1),
        (b"9223372036854775807", i64::MAX), (b"-9223372036854775808", i64::MIN)] {
        assert_eq!(php_integer_key(bytes), Some(expected));
        assert_eq!(classify_string_key(SemanticProfile::PERMISSIVE, bytes), StringKey::Integer(expected));
    }
    for bytes in [b"-0".as_slice(), b"01", b"+1", b"1\0", b"", b"9223372036854775808", b"-9223372036854775809"] {
        assert_eq!(php_integer_key(bytes), None);
    }
}

/// Arithmetic reports overflow without changing the successful result kind.
#[test]
fn strict_scalar_checked_integer_boundaries() {
    use IntegerOp::*;
    assert_eq!(checked_integer(Add, i64::MAX, 1), Err(NumericFailure::Overflow));
    assert_eq!(checked_integer(Sub, i64::MIN, 1), Err(NumericFailure::Overflow));
    assert_eq!(checked_integer(Mul, i64::MIN, -1), Err(NumericFailure::Overflow));
    assert_eq!(checked_integer(Div, i64::MIN, -1), Err(NumericFailure::Overflow));
    assert_eq!(checked_integer(Div, 1, 0), Err(NumericFailure::DivisionByZero));
    assert_eq!(checked_integer(Mod, i64::MIN, -1), Ok(0));
    assert_eq!(checked_integer(Pow, 2, 63), Err(NumericFailure::Overflow));
    assert_eq!(checked_integer(Pow, -2, 63), Ok(i64::MIN));
    assert_eq!(checked_integer(Pow, -1, i64::MAX), Ok(-1));
    assert_eq!(checked_integer(Pow, 1, -1), Err(NumericFailure::NegativeExponent));
    assert_eq!(checked_integer(Pow, 0, 0), Ok(1));
}

/// Explicit casts distinguish exact integer strings from rounded floating-point values.
#[test]
fn strict_scalar_checked_cast_ranges_and_invalid_values() {
    assert_eq!(checked_string_to_int("9223372036854775807"), Ok(i64::MAX));
    assert_eq!(checked_string_to_int("-9223372036854775808"), Ok(i64::MIN));
    assert_eq!(checked_string_to_int("  -12.75  "), Ok(-12));
    assert_eq!(checked_string_to_int("9007199254740993.99"), Ok(9_007_199_254_740_993));
    assert_eq!(checked_string_to_int("9223372036854775807.99"), Ok(i64::MAX));
    assert_eq!(checked_string_to_int("-9223372036854775808.99"), Ok(i64::MIN));
    assert_eq!(checked_string_to_int("-9223372036854775809"), Err(NumericFailure::Overflow));
    assert_eq!(checked_string_to_int("1e999999999999999999999"), Err(NumericFailure::Overflow));
    assert_eq!(checked_string_to_int("-1e-999999999999999999999"), Ok(0));
    assert_eq!(checked_string_to_int("0.000e999999999999999999999"), Ok(0));
    assert_eq!(checked_string_to_int(".001e3"), Ok(1));
    for value in ["NaN", "inf", "1e", ".", "+", "", "1.2.3", "1e2tail"] {
        assert_eq!(checked_string_to_int(value), Err(NumericFailure::InvalidString));
    }
    assert_eq!(checked_string_to_int("12tail"), Err(NumericFailure::InvalidString));
    assert_eq!(checked_float_to_int(i64::MAX as f64), Err(NumericFailure::Overflow));
    assert_eq!(checked_float_to_int(i64::MIN as f64), Ok(i64::MIN));
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(checked_float_to_int(value), Err(NumericFailure::Overflow));
    }
}
