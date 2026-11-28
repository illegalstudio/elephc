//! Purpose:
//! Preserves unsigned integer-literal magnitude until unary syntax determines its sign.
//!
//! Called from:
//! - Compiler and eval lexers under checked integer-literal semantics.
//!
//! Key details:
//! - The magnitude of i64::MIN is a distinct candidate, never a rounded float.

/// Classification of unsigned digits after a lexer removes radix prefixes and separators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerMagnitude {
    /// An ordinary nonnegative signed integer.
    Signed(i64),
    /// Exactly 2^63, legal only as the direct magnitude of unary minus.
    MinimumMagnitude,
    /// Valid digits whose magnitude cannot fit either signed form.
    Overflow,
    /// Empty, signed, malformed digits, or an unsupported radix.
    Invalid,
}

/// Classifies exact digits without interpreting an integer spelling through floating point.
pub fn classify_integer_digits(digits: &str, radix: u32) -> IntegerMagnitude {
    if !(2..=36).contains(&radix) || digits.is_empty() { return IntegerMagnitude::Invalid; }
    let mut magnitude = Some(0_u64);
    for character in digits.chars() {
        let Some(digit) = character.to_digit(radix) else { return IntegerMagnitude::Invalid; };
        magnitude = magnitude.and_then(|value| value.checked_mul(u64::from(radix)))
            .and_then(|value| value.checked_add(u64::from(digit)));
    }
    match magnitude {
        Some(value) if value <= i64::MAX as u64 => IntegerMagnitude::Signed(value as i64),
        Some(value) if value == 1_u64 << 63 => IntegerMagnitude::MinimumMagnitude,
        _ => IntegerMagnitude::Overflow,
    }
}
