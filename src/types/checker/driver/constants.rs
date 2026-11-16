//! Purpose:
//! Makes pure global constant types available before class schemas validate defaults.
//!
//! Called from:
//! - The checker driver before collecting class and interface metadata.
//!
//! Key details:
//! - Only direct const declarations and known constant expression trees participate.
//! - Ordinary checking still validates declarations, visibility and unresolved references.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind, Program, StmtKind};
use crate::types::TypeEnv;
use super::Checker;

/// Resolves declaration types to a bounded fixed point without evaluating runtime statements.
pub(super) fn prescan(checker: &mut Checker, program: &Program) {
    let mut pending = program.iter().filter_map(|statement| match &statement.kind {
        StmtKind::ConstDecl { name, value } => Some((name, value)),
        _ => None,
    }).collect::<Vec<_>>();
    for _ in 0..128 {
        let before = pending.len();
        pending.retain(|(name, value)| {
            if checker.constants.contains_key(*name) { return false; }
            if !known_tree(checker, value) { return true; }
            if let Ok(ty) = checker.infer_type(value, &TypeEnv::new()) {
                checker.constants.insert((*name).clone(), ty);
                return false;
            }
            true
        });
        if pending.len() == before { break; }
    }
}

/// Excludes calls, class lookups and every expression requiring incomplete declaration metadata.
fn known_tree(checker: &Checker, expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_) | ExprKind::Null => true,
        ExprKind::ConstRef(name) => checker.constants.contains_key(name.as_str()),
        ExprKind::Negate(value) | ExprKind::Not(value) | ExprKind::BitNot(value)
            | ExprKind::Spread(value) | ExprKind::Cast { expr: value, .. } => known_tree(checker, value),
        ExprKind::BinaryOp { left, right, .. } => known_tree(checker, left) && known_tree(checker, right),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } =>
            known_tree(checker, value) && known_tree(checker, default),
        ExprKind::Ternary { condition, then_expr, else_expr } => known_tree(checker, condition)
            && known_tree(checker, then_expr) && known_tree(checker, else_expr),
        ExprKind::ArrayLiteral(values) => values.iter().all(|value| known_tree(checker, value)),
        ExprKind::ArrayLiteralAssoc(values) => values.iter().all(|(key, value)| known_tree(checker, key) && known_tree(checker, value)),
        ExprKind::ArrayLiteralMixed(entries) => entries.iter().all(|entry| match entry {
            ArrayEntry::Value(value) | ArrayEntry::Spread(value) => known_tree(checker, value),
            ArrayEntry::Keyed(key, value) => known_tree(checker, key) && known_tree(checker, value),
        }),
        _ => false,
    }
}
