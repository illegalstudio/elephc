//! Purpose:
//! Folds a spread-free `ArrayLiteralMixed` into the `ArrayLiteralAssoc` it stands for, with
//! every implicit key made explicit under the active PHP profile.
//!
//! Called from:
//! - `crate::codegen::literal_defaults` (property defaults and their nested containers).
//! - `crate::codegen::const_default_values` (parameter defaults read by the callable invoker).
//! - `crate::codegen::lower_inst::objects::reflection` (Reflection default metadata).
//!
//! Key details:
//! - The parser keeps a literal such as `[-5 => "a", "b"]` as `ArrayLiteralMixed` because the
//!   bare entry's key depends on the profile: PHP 8.2 restarts at 0 after a negative integer key,
//!   PHP 8.3+ continues at `max + 1`. Every consumer that only knows the list and hash spellings
//!   folds it here first, so a default reflects and materializes exactly like the hash literal
//!   the profile makes it.
//! - Only literal keys are folded. A key the fold cannot classify at compile time (a constant,
//!   an expression) could be an integer that moves the next free slot, so the fold declines and
//!   the caller keeps its existing answer for the unfolded literal.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind};

/// Returns the `ArrayLiteralAssoc` form of a spread-free `ArrayLiteralMixed`, or `None` when
/// `expr` is not one, holds a spread, or has a key the fold cannot classify.
pub(crate) fn fold_spreadless_mixed_array_literal(expr: &ExprKind) -> Option<ExprKind> {
    let ExprKind::ArrayLiteralMixed(entries) = expr else {
        return None;
    };
    let php_version = crate::codegen::compile_php_version();
    let mut max_int: Option<i64> = None;
    let mut pairs = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            ArrayEntry::Spread(_) => return None,
            ArrayEntry::Keyed(key, value) => {
                if let Some(int_key) = literal_int_key(key)? {
                    max_int = Some(max_int.map_or(int_key, |max| max.max(int_key)));
                }
                pairs.push((key.clone(), value.clone()));
            }
            ArrayEntry::Value(value) => {
                let key = match max_int {
                    None => 0,
                    Some(max) if max < 0 && php_version < crate::php_version::PhpVersion::Php83 => 0,
                    Some(max) => max.checked_add(1)?,
                };
                max_int = Some(key);
                pairs.push((Expr::new(ExprKind::IntLiteral(key), value.span), value.clone()));
            }
        }
    }
    Some(ExprKind::ArrayLiteralAssoc(pairs))
}

/// Classifies one literal array key: `Some(Some(n))` for an integer key `n`, `Some(None)` for a
/// string key, and `None` when the key is not a literal the fold understands.
fn literal_int_key(key: &Expr) -> Option<Option<i64>> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(Some(*value)),
        ExprKind::BoolLiteral(value) => Some(Some(i64::from(*value))),
        ExprKind::FloatLiteral(value) => Some(Some(if value.is_finite() { *value as i64 } else { 0 })),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(Some),
            ExprKind::FloatLiteral(value) => {
                let negated = -*value;
                Some(Some(if negated.is_finite() { negated as i64 } else { 0 }))
            }
            _ => None,
        },
        ExprKind::StringLiteral(value) => {
            if crate::types::is_php_integer_array_key(value) {
                value.parse::<i64>().ok().map(Some)
            } else {
                Some(None)
            }
        }
        ExprKind::Null => Some(None),
        _ => None,
    }
}
