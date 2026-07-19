//! Purpose:
//! Shared scalar type helpers used by eval builtins and dynamic conversions.
//!
//! Called from:
//! - `crate::interpreter::builtins::types` and dynamic value conversion paths.
//!
//! Key details:
//! - PHP-visible symbol/introspection builtins live in their focused symbol
//!   builtin files; this module keeps cross-domain scalar predicates only.

use super::super::super::*;

/// Returns the PHP-visible type name for a concrete eval runtime tag.
pub(in crate::interpreter) fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Matches the static backend's legacy ASCII numeric-string scan.
pub(in crate::interpreter) fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}
