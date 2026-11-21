//! Purpose:
//! Tests exact literal magnitudes independently of either frontend token representation.
//!
//! Called from:
//! - The focused elephc-semantics integration test target.
//!
//! Key details:
//! - Decimal and radix spellings share the same signed boundary without float rounding.

use elephc_semantics::integer_literals::{classify_integer_digits, IntegerMagnitude};

/// The signed boundary is exact in every supported PHP integer base.
#[test]
fn strict_integer_literal_signed_boundaries_are_exact_in_each_radix() {
    for (digits, radix, expected) in [
        ("9223372036854775807", 10, IntegerMagnitude::Signed(i64::MAX)),
        ("9223372036854775808", 10, IntegerMagnitude::MinimumMagnitude),
        ("9223372036854775809", 10, IntegerMagnitude::Overflow),
        ("7fffffffffffffff", 16, IntegerMagnitude::Signed(i64::MAX)),
        ("8000000000000000", 16, IntegerMagnitude::MinimumMagnitude),
        ("8000000000000001", 16, IntegerMagnitude::Overflow),
        ("1000000000000000000000", 8, IntegerMagnitude::MinimumMagnitude),
        ("1000000000000000000000000000000000000000000000000000000000000000", 2, IntegerMagnitude::MinimumMagnitude),
        ("18446744073709551616", 10, IntegerMagnitude::Overflow),
        ("00000000000000000000000000000000000001", 10, IntegerMagnitude::Signed(1)),
    ] { assert_eq!(classify_integer_digits(digits, radix), expected, "{digits}, radix={radix}"); }
}

/// Scanners cannot accidentally use the helper to accept signs or malformed digit streams.
#[test]
fn strict_integer_literal_invalid_digits_and_radices_are_rejected() {
    for (digits, radix) in [("", 10), ("+1", 10), ("-1", 10), ("1_0", 10), ("8", 8), ("999999999999999999999999999999z", 10), ("1", 1), ("1", 37)] {
        assert_eq!(classify_integer_digits(digits, radix), IntegerMagnitude::Invalid);
    }
}
