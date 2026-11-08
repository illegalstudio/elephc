//! Purpose:
//! Folds an enum case's backing-value expression to the literal it denotes.
//!
//! Called from:
//! - `super::enums` while enum case metadata is collected.
//!
//! Key details:
//! - PHP 8.2 allows a constant expression as a backed case's value, not only a literal:
//!   `case Focused = Glyphs::SELECTION_BAR;`, `case Ns = self::PREFIX . 'ns';`, `case Low = -1;`.
//! - Classes and interfaces are built before enums, so a class or interface constant the value
//!   names can be read from its `ClassInfo` / `InterfaceInfo` and folded in turn.
//! - Anything that is not a literal, a negated integer, a concatenation or a scoped constant of a
//!   built class-like stays unfolded, and the caller reports the existing diagnostic.

use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver};

use super::super::Checker;

/// A folded backing value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FoldedBacking {
    Int(i64),
    Str(String),
}

/// Constant chains deeper than this are treated as unfoldable rather than walked further.
const MAX_FOLD_DEPTH: usize = 32;

/// Folds `expr` to an integer or string literal, or returns `None` when it is not a constant
/// expression this pass can evaluate.
pub(super) fn fold_backing_value(checker: &Checker, expr: &Expr) -> Option<FoldedBacking> {
    fold(checker, expr, 0)
}

/// Recursive worker for [`fold_backing_value`], bounded by [`MAX_FOLD_DEPTH`].
fn fold(checker: &Checker, expr: &Expr, depth: usize) -> Option<FoldedBacking> {
    if depth > MAX_FOLD_DEPTH {
        return None;
    }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(FoldedBacking::Int(*value)),
        ExprKind::StringLiteral(value) => Some(FoldedBacking::Str(value.clone())),
        ExprKind::Negate(inner) => match fold(checker, inner, depth + 1)? {
            FoldedBacking::Int(value) => value.checked_neg().map(FoldedBacking::Int),
            FoldedBacking::Str(_) => None,
        },
        ExprKind::BinaryOp { left, op: BinOp::Concat, right } => {
            let left = fold(checker, left, depth + 1)?;
            let right = fold(checker, right, depth + 1)?;
            Some(FoldedBacking::Str(format!("{}{}", as_string(&left), as_string(&right))))
        }
        ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(class), name } => {
            let value = class_like_constant(checker, class.as_str(), name)?;
            fold(checker, value, depth + 1)
        }
        _ => None,
    }
}

/// PHP's string conversion of a folded operand inside a concatenation.
fn as_string(value: &FoldedBacking) -> String {
    match value {
        FoldedBacking::Int(value) => value.to_string(),
        FoldedBacking::Str(value) => value.clone(),
    }
}

/// The declared value of `class::name` on a built class or interface, matched case-insensitively
/// on the class-like name as PHP does.
fn class_like_constant<'a>(checker: &'a Checker, class: &str, name: &str) -> Option<&'a Expr> {
    let key = php_symbol_key(class.trim_start_matches('\\'));
    let class_constant = checker
        .classes
        .iter()
        .find(|(declared, _)| php_symbol_key(declared) == key)
        .and_then(|(_, info)| info.constants.get(name));
    class_constant.or_else(|| {
        checker
            .interfaces
            .iter()
            .find(|(declared, _)| php_symbol_key(declared) == key)
            .and_then(|(_, info)| info.constants.get(name))
    })
}
