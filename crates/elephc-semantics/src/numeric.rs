//! Purpose:
//! Evaluates checked integer arithmetic and explicit numeric conversion boundaries.
//!
//! Called from:
//! - Profile-aware constant folding and eval/runtime semantic adapters.
//!
//! Key details:
//! - Successful integer operations always retain signed 64-bit integer semantics.
//! - Failure is typed data; callers decide source diagnostics or exception delivery.

/// Shared text for catchable checked integer arithmetic failures.
pub const INTEGER_OVERFLOW_MESSAGE: &str = "--strict-overflow: integer arithmetic overflow";

/// Version-one checked numeric callback selectors, independent of PHP implementation operations.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedNumericOp { Add = 0, Sub = 1, Mul = 2, Pow = 3, Abs = 4 }

/// Integer arithmetic with an integer result contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerOp { Add, Sub, Mul, Div, Mod, Pow }

/// A numeric operation could not satisfy its checked integer contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericFailure { Overflow, DivisionByZero, NegativeExponent, InvalidString }

/// Evaluates an integer operation without promotion or wrapping on arithmetic overflow.
pub fn checked_integer(op: IntegerOp, left: i64, right: i64) -> Result<i64, NumericFailure> {
    let result = match op {
        IntegerOp::Add => left.checked_add(right),
        IntegerOp::Sub => left.checked_sub(right),
        IntegerOp::Mul => left.checked_mul(right),
        IntegerOp::Div | IntegerOp::Mod if right == 0 => return Err(NumericFailure::DivisionByZero),
        IntegerOp::Div => left.checked_div(right),
        IntegerOp::Mod if right == -1 => Some(0),
        IntegerOp::Mod => left.checked_rem(right),
        IntegerOp::Pow => return checked_power(left, right),
    };
    result.ok_or(NumericFailure::Overflow)
}

/// Evaluates exponentiation by squaring with a nonnegative signed 64-bit exponent.
fn checked_power(mut base: i64, exponent: i64) -> Result<i64, NumericFailure> {
    if exponent < 0 { return Err(NumericFailure::NegativeExponent); }
    let mut exponent = exponent as u64;
    let mut result = 1i64;
    while exponent != 0 {
        if exponent & 1 != 0 { result = result.checked_mul(base).ok_or(NumericFailure::Overflow)?; }
        exponent >>= 1;
        if exponent != 0 { base = base.checked_mul(base).ok_or(NumericFailure::Overflow)?; }
    }
    Ok(result)
}

/// Truncates an explicitly converted finite float only when its value fits signed 64-bit.
pub fn checked_float_to_int(value: f64) -> Result<i64, NumericFailure> {
    if !value.is_finite() || value < -9_223_372_036_854_775_808.0
        || value >= 9_223_372_036_854_775_808.0 { return Err(NumericFailure::Overflow); }
    Ok(value.trunc() as i64)
}

/// Parses a complete numeric string for an explicit int cast, preserving exact integer strings.
pub fn checked_string_to_int(value: &str) -> Result<i64, NumericFailure> {
    let value = value.trim_matches(|ch: char| ch.is_ascii_whitespace());
    let (negative, value) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
    let integer_digits = cursor;
    if bytes.get(cursor) == Some(&b'.') {
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) { cursor += 1; }
    }
    let mantissa = &bytes[..cursor];
    if !mantissa.iter().any(u8::is_ascii_digit) { return Err(NumericFailure::InvalidString); }
    let mut exponent = 0i64;
    if matches!(bytes.get(cursor), Some(b'e' | b'E')) {
        cursor += 1;
        let exponent_negative = bytes.get(cursor) == Some(&b'-');
        if matches!(bytes.get(cursor), Some(b'+' | b'-')) { cursor += 1; }
        let exponent_start = cursor;
        while let Some(digit @ b'0'..=b'9') = bytes.get(cursor) {
            exponent = exponent.saturating_mul(10).saturating_add(i64::from(digit - b'0'));
            cursor += 1;
        }
        if cursor == exponent_start { return Err(NumericFailure::InvalidString); }
        if exponent_negative { exponent = -exponent; }
    }
    if cursor != bytes.len() { return Err(NumericFailure::InvalidString); }
    let decimal_position = (integer_digits as i64).saturating_add(exponent);
    if decimal_position <= 0 { return Ok(0); }
    // Accumulate in the negative range so the minimum signed integer remains representable.
    let mut result = 0i64;
    let mut remaining = decimal_position;
    for digit in mantissa.iter().filter(|digit| **digit != b'.').take(decimal_position as usize) {
        result = result.checked_mul(10).and_then(|value| value.checked_sub(i64::from(digit - b'0')))
            .ok_or(NumericFailure::Overflow)?;
        remaining -= 1;
    }
    if result == 0 { return Ok(0); }
    // A nonzero integer cannot acquire more than 18 trailing zeroes and still fit.
    if remaining > 18 { return Err(NumericFailure::Overflow); }
    for _ in 0..remaining { result = result.checked_mul(10).ok_or(NumericFailure::Overflow)?; }
    if negative { Ok(result) } else { result.checked_neg().ok_or(NumericFailure::Overflow) }
}
