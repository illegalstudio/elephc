//! Purpose:
//! Classifies string keys without depending on AST or runtime storage.
//!
//! Called from:
//! - Compiler array-shape inference and eval key adapters.
//!
//! Key details:
//! - Exact keys preserve bytes, length, and string kind, including embedded NUL.

use crate::{SemanticProfile, StrictFeature};

/// Shared diagnostic for original values outside the exact array-key domain.
pub const EXACT_KEY_MESSAGE: &str = "--strict-array-keys requires int|string array keys without conversion";

/// The semantic key obtained from a borrowed string spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StringKey<'a> {
    /// A PHP-normalized signed integer key.
    Integer(i64),
    /// Original bytes, still a string regardless of their numeric spelling.
    String(&'a [u8]),
}

/// Classifies one string under the selected array-key policy.
pub fn classify_string_key(profile: SemanticProfile, value: &[u8]) -> StringKey<'_> {
    if !profile.contains(StrictFeature::ArrayKeys) {
        if let Some(integer) = php_integer_key(value) { return StringKey::Integer(integer); }
    }
    StringKey::String(value)
}

/// Parses PHP's canonical integer-string key grammar and signed 64-bit range.
pub fn php_integer_key(value: &[u8]) -> Option<i64> {
    if value == b"0" { return Some(0); }
    let digits = value.strip_prefix(b"-").unwrap_or(value);
    if digits.is_empty() || !matches!(digits[0], b'1'..=b'9')
        || !digits.iter().all(u8::is_ascii_digit) { return None; }
    std::str::from_utf8(value).ok()?.parse().ok()
}

/// Scans the numeric key spelling accepted by PHP simple string interpolation.
///
/// The returned length excludes the closing bracket. Canonical decimal spellings become
/// integers, except the negative `2^63` magnitude, which PHP exposes to `ArrayAccess` as text.
/// Prefixed, underscored, noncanonical, and out-of-range spellings retain their source bytes.
pub fn scan_simple_interpolation_numeric_key(source: &[u8]) -> Option<(usize, StringKey<'_>)> {
    let sign_len = usize::from(source.first() == Some(&b'-'));
    let unsigned = source.get(sign_len..)?;
    if !unsigned.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let body_len = interpolation_numeric_body_len(unsigned);
    let len = sign_len + body_len;
    let spelling = &source[..len];
    Some((len, simple_interpolation_key(spelling)))
}

/// Returns the numeric body width, including valid radix prefixes and separators.
fn interpolation_numeric_body_len(source: &[u8]) -> usize {
    let radix = match source {
        [b'0', b'x' | b'X', digit, ..] if digit.is_ascii_hexdigit() => Some(16),
        [b'0', b'o' | b'O', b'0'..=b'7', ..] => Some(8),
        [b'0', b'b' | b'B', b'0' | b'1', ..] => Some(2),
        _ => None,
    };
    match radix {
        Some(radix) => 2 + interpolation_digits_len(&source[2..], radix),
        None => interpolation_digits_len(source, 10),
    }
}

/// Returns a digit run while accepting one underscore only between valid digits.
fn interpolation_digits_len(source: &[u8], radix: u32) -> usize {
    let valid = |byte: u8| char::from(byte).to_digit(radix).is_some();
    if !source.first().copied().is_some_and(valid) {
        return 0;
    }
    let mut end = 1;
    loop {
        if source.get(end).copied().is_some_and(valid) {
            end += 1;
        } else if source.get(end) == Some(&b'_')
            && source.get(end + 1).copied().is_some_and(valid)
        {
            end += 2;
        } else {
            return end;
        }
    }
}

/// Applies PHP interpolation's narrower signed-integer conversion to one complete spelling.
fn simple_interpolation_key(spelling: &[u8]) -> StringKey<'_> {
    let Some(magnitude) = spelling.strip_prefix(b"-") else {
        return php_integer_key(spelling)
            .map(StringKey::Integer)
            .unwrap_or(StringKey::String(spelling));
    };
    let canonical = magnitude.first().is_some_and(|digit| matches!(digit, b'1'..=b'9'))
        && magnitude.iter().all(u8::is_ascii_digit);
    if canonical {
        if let Ok(text) = std::str::from_utf8(magnitude) {
            if let Ok(value) = text.parse::<i64>() {
                return StringKey::Integer(-value);
            }
        }
    }
    StringKey::String(spelling)
}
