//! Purpose:
//! Shared helper functions for the JSON builtins
//! (`json_encode`, `json_decode`, `json_validate`, `unserialize`).
//! Relocated from `src/types/checker/builtins/system.rs` into the builtin
//! registry home area so all JSON homes can import them from one place.
//!
//! Called from:
//! - `crate::builtins::system::json_encode` (check hook)
//! - `crate::builtins::system::json_decode` (check hook)
//! - `crate::builtins::system::json_validate` (check hook)
//! - `crate::builtins::system::unserialize` (check hook)
//!
//! Key details:
//! - `is_json_string_arg_type` accepts scalars and Mixed (not arrays/objects).
//! - `is_json_associative_arg_type` accepts bool-compatible types and Mixed.
//! - `json_static_int_value` folds literals, known JSON constants, and bitwise ops.

use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::json_constants::JSON_INT_CONSTANTS;
use crate::types::PhpType;

/// Returns `true` if `ty` is a valid type for the JSON string argument in
/// `json_decode` / `json_validate` / `json_encode` (scalar types and `Mixed`).
pub(crate) fn is_json_string_arg_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Str
        | PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Mixed => true,
        PhpType::Union(types) => types.iter().all(is_json_string_arg_type),
        _ => false,
    }
}

/// Returns `true` if `ty` is a valid type for the associative argument in
/// `json_decode` (bool-compatible types plus `Mixed`).
pub(crate) fn is_json_associative_arg_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Mixed => true,
        PhpType::Union(types) => types.iter().all(is_json_associative_arg_type),
        _ => false,
    }
}

/// Attempts to evaluate an expression as a static integer at compile time.
/// Supports literals, known constants, negation, and bitwise ops.
/// Returns `Some(value)` if the expression is statically computable, `None` otherwise.
pub(crate) fn json_static_int_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => JSON_INT_CONSTANTS
            .iter()
            .find_map(|(constant, value)| (*constant == name.as_str()).then_some(*value)),
        ExprKind::Negate(inner) => json_static_int_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = json_static_int_value(left)?;
            let right = json_static_int_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
