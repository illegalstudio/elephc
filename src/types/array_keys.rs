//! Purpose:
//! Normalizes PHP array key types from literal expressions and inferred value types.
//! Captures PHP integer-string key coercion for list and associative array typing.
//!
//! Called from:
//! - `crate::types::checker::inference`
//! - `crate::types::checker::builtins`
//!
//! Key details:
//! - Key normalization must match PHP because it affects array shape merging and mixed associative-array values.

use crate::parser::ast::{Expr, ExprKind};

use super::PhpType;

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

pub(crate) fn merge_array_key_types(left: PhpType, right: PhpType) -> PhpType {
    if left == right {
        left
    } else {
        PhpType::Mixed
    }
}

pub(crate) fn array_key_type_from_value_type(raw_ty: PhpType) -> PhpType {
    match raw_ty {
        PhpType::Int | PhpType::Bool | PhpType::Float => PhpType::Int,
        PhpType::Str => PhpType::Mixed,
        other => other,
    }
}

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
