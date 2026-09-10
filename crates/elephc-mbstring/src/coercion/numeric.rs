//! Purpose:
//! Parses PHP decimal syntax and representable integer-parameter values.
//!
//! Called from:
//! - The shared mbstring parameter and numeric-entity map coercion planners.
//!
//! Key details:
//! - Complete parameters reject junk and NUL tails; maps may consume warned numeric prefixes.
//! - Direct integer parsing preserves 64-bit endpoints before floating-point fallback.
//! - Integer parameters reject out-of-range and NaN floats; map casting has a separate policy.

/// Truncates a double only when PHP's signed 64-bit parameter conversion can represent it.
pub(super) fn float_integer(bits: u64) -> Option<i64> {
    let value = f64::from_bits(bits);
    (value >= i64::MIN as f64 && value < -(i64::MIN as f64)).then(|| value as i64)
}

/// A parsed decimal prefix before the caller applies its integer-conversion policy.
pub(super) enum Numeric { Integer(i64), Float(f64) }

/// Parses complete numeric strings for parameters, retaining the representable integer restriction.
pub(super) fn numeric_integer(bytes: &[u8]) -> Option<(i64, bool)> {
    let (number, consumed) = numeric_prefix(bytes)?;
    if !bytes[consumed..].iter().copied().all(whitespace) { return None; }
    match number {
        Numeric::Integer(value) => Some((value, false)),
        Numeric::Float(value) => {
            let integer = float_integer(value.to_bits())?;
            Some((integer, value != integer as f64))
        },
    }
}

/// Reads PHP's leading decimal number and returns the byte offset before trailing input.
pub(super) fn numeric_prefix(bytes: &[u8]) -> Option<(Numeric, usize)> {
    let mut start = 0;
    while bytes.get(start).copied().is_some_and(whitespace) { start += 1; }
    let mut cursor = start + usize::from(matches!(bytes.get(start), Some(b'+' | b'-')));
    let first_digit = cursor;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
    let mut digits = cursor - first_digit;
    let mut floating = false;
    if bytes.get(cursor) == Some(&b'.') {
        floating = true;
        cursor += 1;
        let first_digit = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
        digits += cursor - first_digit;
    }
    if digits == 0 { return None; }
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        let exponent = cursor;
        cursor += 1;
        cursor += usize::from(matches!(bytes.get(cursor), Some(b'+' | b'-')));
        let first_digit = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
        if cursor == first_digit { cursor = exponent; } else { floating = true; }
    }
    let text = std::str::from_utf8(&bytes[start..cursor]).ok()?;
    if !floating {
        if let Ok(value) = text.parse::<i64>() { return Some((Numeric::Integer(value), cursor)); }
    }
    Some((Numeric::Float(text.parse::<f64>().ok()?), cursor))
}

/// Includes PHP's vertical-tab whitespace, which Rust's ASCII whitespace predicate excludes.
pub(super) fn whitespace(byte: u8) -> bool { matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12) }
