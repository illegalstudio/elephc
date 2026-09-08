//! Purpose:
//! Defines binary operator variants produced by expression parsing.
//! Provides the AST-level operator vocabulary consumed by type checking, optimization, and codegen.
//!
//! Called from:
//! - `crate::parser::expr::pratt` and assignment lowering helpers.
//!
//! Key details:
//! - Variants must stay aligned with lexer tokens and PHP precedence rules in the Pratt table.

use super::{Expr, ExprKind};

// --- Operators ---

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Pow,
    And,
    Or,
    Xor,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Spaceship,
    NullCoalesce,
}

/// Recognizes the direct-AST `operand * 1` marker emitted only for PHP unary plus.
pub(crate) fn is_synthetic_unary_plus(op: &BinOp, right: &Expr) -> bool {
    matches!(op, BinOp::Mul)
        && matches!(right.kind, ExprKind::IntLiteral(1))
        && !right.span.is_from_source()
}
