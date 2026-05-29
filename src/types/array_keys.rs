//! Purpose:
//! Normalizes PHP array key types from literal expressions and inferred value types.
//! Captures PHP integer-string key coercion plus literal string-offset parsing.
//!
//! Called from:
//! - `crate::types::checker::inference`
//! - `crate::types::checker::builtins`
//! - `crate::codegen::expr::arrays::access::string_offset`
//!
//! Key details:
//! - Key and offset normalization must match PHP because it affects array shapes and string access.

use crate::parser::ast::{Expr, ExprKind};

use super::PhpType;

/// Determines the normalized PHP type for an array key expression.
///
/// PHP integer-string key coercion: numeric strings like `"123"` become `PhpType::Int`
/// when used as array keys. Float and boolean literals also coerce to integers.
/// Non-numeric strings remain `PhpType::Str`. When `raw_ty` is `PhpType::Str` and the
/// expression is not a string literal, returns `PhpType::Mixed` to indicate ambiguous key type.
///
/// Returns the key type to use during type checking and shape inference.
pub(crate) fn normalized_array_key_type(expr: &Expr, raw_ty: PhpType) -> PhpType {
    match &expr.kind {
        ExprKind::IntLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::FloatLiteral(_) => {
            PhpType::Int
        }
        ExprKind::StringLiteral(value) => {
            if is_php_integer_array_key(value) {
                PhpType::Int
            } else {
                PhpType::Str
            }
        }
        _ => match raw_ty {
            PhpType::Int | PhpType::Bool | PhpType::Float => PhpType::Int,
            PhpType::Str => PhpType::Mixed,
            other => other,
        },
    }
}

/// Returns true if a static array key forces hash-map (associative) storage in PHP.
///
/// An integer key forces hash storage unless it is exactly `0`.
/// Boolean and float literals are tested after their PHP integer-key cast.
/// A string key forces hash storage if it is a valid PHP integer string and not `"0"`.
/// Other expressions (variables, function calls, etc.) do not force hash storage
/// and may use packed array optimization.
///
/// Used during array shape inference to determine whether a statically-known
/// key requires associative lookup semantics.
pub(crate) fn static_array_key_forces_hash_storage(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(value) => *value != 0,
        ExprKind::BoolLiteral(value) => *value,
        ExprKind::FloatLiteral(value) => (*value as i64) != 0,
        ExprKind::StringLiteral(value) => is_php_integer_array_key(value) && value != "0",
        _ => false,
    }
}

/// Merges two array key types from adjacent elements into a unified key type.
///
/// If both sides have the same type, returns that type. Otherwise returns `PhpType::Mixed`
/// to indicate heterogeneous key types require associative storage.
///
/// Used when inferring array shape from initializer lists with multiple elements.
pub(crate) fn merge_array_key_types(left: PhpType, right: PhpType) -> PhpType {
    if left == right {
        left
    } else {
        PhpType::Mixed
    }
}

/// Infers the array key type from a value type when no explicit key is provided.
///
/// PHP uses this rule: `array_values()` on integers, booleans, or floats yields integer keys;
/// strings yield `PhpType::Mixed` keys (ambiguous integer-string); other types preserve
/// their own type as the key type.
///
/// Returns the key type for an array element when the key expression is absent.
pub(crate) fn array_key_type_from_value_type(raw_ty: PhpType) -> PhpType {
    match raw_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => PhpType::Int,
        PhpType::Str => PhpType::Mixed,
        other => other,
    }
}

/// Returns true if `value` is a PHP-valid integer array key.
///
/// A valid PHP integer string is a decimal integer (optionally signed) that fits in a signed
/// 64-bit integer (`-9223372036854775808` to `9223372036854775807`). The string must contain
/// no leading zeros (except `"0"` itself), no leading `+`, no whitespace, and only digits
/// possibly prefixed by a single `-` or `+`.
///
/// Used to determine whether a string literal should be treated as an integer key.
pub(crate) fn is_php_integer_array_key(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let (start, negative) = if bytes[0] == b'-' {
        if bytes.len() == 1 || bytes[1] == b'0' {
            return false;
        }
        (1, true)
    } else {
        if bytes[0] == b'0' {
            return false;
        }
        (0, false)
    };
    let digits = &bytes[start..];
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let limit = if negative {
        "9223372036854775808"
    } else {
        "9223372036854775807"
    };
    digits.len() < limit.len() || (digits.len() == limit.len() && digits <= limit.as_bytes())
}

/// Parses a string literal as a PHP string-offset index, returning the integer value.
///
/// Accepts strings like `"42"`, `"+123"`, `"-7"`, with optional ASCII whitespace trimming.
/// Returns `None` if the string is empty, contains non-digit characters (except leading `+`/`-`),
/// or the parsed value overflows `i64`.
///
/// Used when lowering string offset access in codegen to determine if a literal offset
/// can be treated as a static integer rather than requiring runtime parsing.
pub(crate) fn parse_php_string_offset_literal(value: &str) -> Option<i64> {
    let trimmed = value.trim_matches(|ch: char| ch.is_ascii_whitespace());
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.as_bytes()[0];
    let digits = if first == b'+' || first == b'-' {
        &trimmed[1..]
    } else {
        trimmed
    };
    if digits.is_empty() || !digits.as_bytes().iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<i64>().ok()
}
