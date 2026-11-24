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
//!   enclosing operators and array reads are folded again (`('n' . C::ONE) === 'n1' ? 'yes' :
//!   'no'`, `['n' . C::ONE][0]`).
//! - A value that folds to a literal of another type is reported by the caller with PHP's case
//!   type mismatch message. A value that does not fold is left to the caller too, which reports
//!   that it cannot evaluate it. The enum's own constants (`self::X`) are not built yet at this
//!   point.

use std::collections::HashMap;

use crate::optimize::fold_constant_expression;
use crate::parser::ast::{ArrayEntry, BinOp, Expr, ExprKind};

use super::super::Checker;
use super::property_default_selectors::bind_constants;

/// A folded backing value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FoldedBacking {
    Int(i64),
    Str(String),
    /// A literal of a type no enum can be backed by, named as PHP's `zend_zval_type_name`
    /// prints it (`float`, `bool`, `null`, `array`), for the case type mismatch error.
    OtherType(&'static str),
}

/// Folds `expr`, declared in enum `owner`, to a literal, or returns `None` when it is not a
/// constant expression this pass can evaluate.
pub(super) fn fold_backing_value(
    checker: &Checker,
    global_constants: &HashMap<String, Expr>,
    owner: &str,
    expr: &Expr,
) -> Option<FoldedBacking> {
    let mut value = expr.clone();
    bind_constants(&mut value, owner, checker, global_constants, 0);
    join_int_concats(&mut value);
    // A joined concatenation can decide a ternary whose skipped branch is not a literal
    // (`('n' . 1) ? 6 : $v`); the forms check already let PHP's compile-time pruning skip it.
    crate::types::deferred_defaults::prune_literal_branches(&mut value);
    // An array literal is an array value once it builds cleanly (empty, keyed and nested ones
    // too). A key PHP refuses or reports before the type check (an array, `null`, a fractional
    // float, an append past `PHP_INT_MAX`) leaves it undecided, as for a ternary condition.
    if matches!(
        value.kind,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) | ExprKind::ArrayLiteralMixed(_)
    ) {
        return crate::types::deferred_defaults::literal_truthiness(&value)
            .map(|_| FoldedBacking::OtherType("array"));
    }
    match value.kind {
        ExprKind::IntLiteral(value) => Some(FoldedBacking::Int(value)),
        ExprKind::StringLiteral(value) => Some(FoldedBacking::Str(value)),
        ExprKind::FloatLiteral(_) => Some(FoldedBacking::OtherType("float")),
        ExprKind::BoolLiteral(_) => Some(FoldedBacking::OtherType("bool")),
        ExprKind::Null => Some(FoldedBacking::OtherType("null")),
        _ => None,
    }
}

/// Joins the `.` concatenations of int and string literals that the shared folder leaves alone
/// (an int converts to its decimal string, as in PHP), children first, then refolds each operator
/// so a comparison, ternary or array read around a joined concatenation folds too
/// (`['n' . C::ONE][0]`).
pub(super) fn join_int_concats(expr: &mut Expr) {
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
        ExprKind::ArrayAccess { array, index } => {
            join_int_concats(array);
            join_int_concats(index);
        }
        ExprKind::ArrayLiteral(items) => items.iter_mut().for_each(join_int_concats),
        ExprKind::ArrayLiteralAssoc(items) => items.iter_mut().for_each(|(key, value)| {
            join_int_concats(key);
            join_int_concats(value);
        }),
        ExprKind::ArrayLiteralMixed(items) => items.iter_mut().for_each(|item| match item {
            ArrayEntry::Value(value) | ArrayEntry::Spread(value) => join_int_concats(value),
            ArrayEntry::Keyed(key, value) => {
                join_int_concats(key);
                join_int_concats(value);
            }
            ArrayEntry::Reference(_) | ArrayEntry::KeyReference(..) => {}
        }),
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
