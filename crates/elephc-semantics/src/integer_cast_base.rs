//! Purpose:
//! Parses explicit integer radix conversions without prefix acceptance or range saturation.
//!
//! Called from:
//! - Checked intval semantic validation, eval and native radix-parser parity tests.
//!
//! Key details:
//! - Decimal base ten retains the exact fractional/exponent cast contract.
//! - Other bases accept complete integer digit runs with PHP's reviewed prefix rules.

use crate::{numeric::checked_string_to_int, numeric_cast::{numeric_cast_failure, NumericCastFailure},
    numeric_string::is_php_whitespace};

/// Checks an explicitly requested string radix without needing the string's value.
pub fn validate_integer_cast_base(base: i64) -> Result<(), NumericCastFailure> {
    if base == 0 || (2..=36).contains(&base) { Ok(()) } else { Err(NumericCastFailure::InvalidBase) }
}

/// Parses a complete string in its explicit radix, preserving both signed integer endpoints.
pub fn checked_string_to_int_base(bytes: &[u8], mut base: i64) -> Result<i64, NumericCastFailure> {
    if base == 10 {
        let text = std::str::from_utf8(bytes).map_err(|_| NumericCastFailure::InvalidString)?;
        return checked_string_to_int(text).map_err(numeric_cast_failure);
    }
    validate_integer_cast_base(base)?;
    let mut rest = bytes;
    while rest.first().is_some_and(|byte| is_php_whitespace(*byte)) { rest = &rest[1..]; }
    while rest.last().is_some_and(|byte| is_php_whitespace(*byte)) { rest = &rest[..rest.len() - 1]; }
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'-' | b'+')) { rest = &rest[1..]; }
    if rest.len() >= 2 && rest[0] == b'0' {
        let marker = rest[1] | 32;
        if marker == b'x' && (base == 0 || base == 16) { base = 16; rest = &rest[2..]; }
        else if marker == b'b' && (base == 0 || base == 2) { base = 2; rest = &rest[2..]; }
    }
    if base == 0 { base = if rest.first() == Some(&b'0') { 8 } else { 10 }; }
    if rest.is_empty() { return Err(NumericCastFailure::InvalidString); }
    let mut value = Some(0i64);
    for byte in rest {
        let digit = match byte {
            b'0'..=b'9' => i64::from(byte - b'0'), b'a'..=b'z' => i64::from(byte - b'a' + 10),
            b'A'..=b'Z' => i64::from(byte - b'A' + 10), _ => return Err(NumericCastFailure::InvalidString),
        };
        if digit >= base { return Err(NumericCastFailure::InvalidString); }
        value = value.and_then(|value| value.checked_mul(base)).and_then(|value| value.checked_sub(digit));
    }
    let value = value.ok_or(NumericCastFailure::Range)?;
    if negative { Ok(value) } else { value.checked_neg().ok_or(NumericCastFailure::Range) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit radices preserve full-width integers, decimal casts and reviewed prefix selection.
    #[test]
    fn strict_integer_cast_base_exact_values_and_prefix_selection() {
        for (bytes, base, expected) in [(b"7fffffffffffffff".as_slice(), 16, i64::MAX),
            (b"-8000000000000000", 16, i64::MIN), (b"+0Xff", 0, 255), (b"0b101", 2, 5),
            (b"0B101", 0, 5), (b"077", 0, 63), (b"077", 10, 77), (b"1e3", 10, 1000),
            (b"1e3", 16, 483), (b"0b10", 16, 2832), (b"z", 36, 35), (b"\x0b-42\x0c", 10, -42),
            (b"9223372036854775807.9", 10, i64::MAX)] {
            assert_eq!(checked_string_to_int_base(bytes, base), Ok(expected), "{bytes:?} base={base}");
        }
    }

    /// Prefixes, invalid digits, trailing junk and range failures remain distinct from bad bases.
    #[test]
    fn strict_integer_cast_base_rejects_partial_and_saturating_values() {
        for (bytes, base) in [(b"".as_slice(), 16), (b"0x", 16), (b"0b", 0), (b"09", 0),
            (b"1e3", 0), (b"0x10", 2), (b"fftail", 16), (b"1 0", 10), (b"1\0", 16), (b"z", 35),
            (b"8000000000000000z", 16)] {
            assert_eq!(checked_string_to_int_base(bytes, base), Err(NumericCastFailure::InvalidString), "{bytes:?} base={base}");
        }
        for base in [i64::MIN, -1, 1, 37, i64::MAX] {
            assert_eq!(checked_string_to_int_base(b"42", base), Err(NumericCastFailure::InvalidBase));
        }
        for (bytes, base) in [(b"8000000000000000".as_slice(), 16), (b"-8000000000000001", 16),
            (b"9223372036854775808", 10), (b"zzzzzzzzzzzzzzzz", 36)] {
            assert_eq!(checked_string_to_int_base(bytes, base), Err(NumericCastFailure::Range));
        }
    }
}
