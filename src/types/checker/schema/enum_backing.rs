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
//!   string literals, so a concatenation with an int operand is joined here, bottom-up, and the
//!   enclosing operators are folded again (`('n' . C::ONE) === 'n1' ? 'yes' : 'no'`).
//! - A value that does not fold to an int or string literal is left to the caller, which reports
//!   the existing diagnostic. The enum's own constants (`self::X`) are not built yet at this point.

use std::collections::HashMap;

use crate::optimize::fold_constant_expression;
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
    join_int_concats(&mut value);
    match value.kind {
        ExprKind::IntLiteral(value) => Some(FoldedBacking::Int(value)),
        ExprKind::StringLiteral(value) => Some(FoldedBacking::Str(value)),
        _ => None,
    }
}

/// Joins the `.` concatenations of int and string literals that the shared folder leaves alone
/// (an int converts to its decimal string, as in PHP), children first, then refolds each operator
/// so a comparison or ternary around a joined concatenation folds too.
fn join_int_concats(expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            join_int_concats(left);
            join_int_concats(right);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            join_int_concats(condition);
            join_int_concats(then_expr);
            join_int_concats(else_expr);
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            join_int_concats(value);
            join_int_concats(default);
        }
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. } => join_int_concats(value),
        _ => return,
    }
    if let ExprKind::BinaryOp { left, op: BinOp::Concat, right } = &expr.kind {
        if let (Some(left), Some(right)) = (concat_operand(left), concat_operand(right)) {
            expr.kind = ExprKind::StringLiteral(left + &right);
            return;
        }
    }
    *expr = fold_constant_expression(expr.clone());
}

/// PHP's string conversion of an int or string literal concatenation operand.
fn concat_operand(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(value.to_string()),
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}
