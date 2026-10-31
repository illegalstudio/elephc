//! Purpose:
//! Provides the two type-expression facts the parser needs without the full type checker.
//!
//! Called from:
//! - Parser append-write lowering.

use crate::parser::ast::{Expr, ExprKind};

/// Returns whether an empty-array key needs hash storage rather than packed indexing.
pub fn empty_array_key_requires_hash_storage(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(value) => *value != 0,
        ExprKind::BoolLiteral(value) => *value,
        ExprKind::FloatLiteral(value) => (*value as i64) != 0,
        ExprKind::StringLiteral(value) => value != "0",
        _ => true,
    }
}
