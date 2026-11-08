//! Purpose:
//! Folds an enum case's backing-value expression to the literal it denotes.
//!
//! Called from:
//! - `super::enums` while enum case metadata is collected.
//!
//! Key details:
//! - PHP 8.2 allows a constant expression as a backed case's value, not only a literal:
//!   `case Focused = Glyphs::SELECTION_BAR;`, `case Ns = PREFIX . 'ns';`, `case Big = 1 << K::SHIFT;`.
//! - Classes and interfaces are built before enums, so the value is bound and folded by the same
//!   `property_default_selectors::bind_constants` pass that resolves property defaults: class and
//!   interface constants (inherited ones included), global `const` declarations and predefined
//!   constants, then the optimizer's PHP-equivalent constant folding. That folder only joins
//!   string literals, so a concatenation with an int operand is joined here.
//! - A value that does not fold to an int or string literal is left to the caller, which reports
//!   the existing diagnostic. The enum's own constants (`self::X`) are not built yet at this point.

use std::collections::HashMap;

use crate::parser::ast::{BinOp, Expr, ExprKind};

use super::super::Checker;
use super::property_default_selectors::bind_constants;

/// A folded backing value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FoldedBacking {
    Int(i64),
    Str(String),
}

/// Folds `expr`, declared in enum `owner`, to an integer or string literal, or returns `None`
/// when it is not a constant expression this pass can evaluate.
pub(super) fn fold_backing_value(
    checker: &Checker,
    global_constants: &HashMap<String, Expr>,
    owner: &str,
    expr: &Expr,
) -> Option<FoldedBacking> {
    let mut value = expr.clone();
    bind_constants(&mut value, owner, checker, global_constants, 0);
    fold_literal(&value)
}

/// Reads a bound expression as a literal, also joining the `.` concatenations of int and string
/// literals that the shared folder leaves alone (an int converts to its decimal string, as in PHP).
fn fold_literal(expr: &Expr) -> Option<FoldedBacking> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(FoldedBacking::Int(*value)),
        ExprKind::StringLiteral(value) => Some(FoldedBacking::Str(value.clone())),
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            let mut joined = concat_operand(fold_literal(left)?);
            joined.push_str(&concat_operand(fold_literal(right)?));
            Some(FoldedBacking::Str(joined))
        }
        _ => None,
    }
}

/// PHP's string conversion of a folded concatenation operand.
fn concat_operand(value: FoldedBacking) -> String {
    match value {
        FoldedBacking::Int(value) => value.to_string(),
        FoldedBacking::Str(value) => value,
    }
}
