//! Purpose:
//! Classifies string keys without depending on AST or runtime storage.
//!
//! Called from:
//! - Compiler array-shape inference and eval key adapters.
//!
//! Key details:
//! - Exact keys preserve bytes, length, and string kind, including embedded NUL.

use crate::{SemanticProfile, StrictFeature};

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
