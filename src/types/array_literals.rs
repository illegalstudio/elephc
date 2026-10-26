//! Purpose:
//! Normalizes spread-free mixed array literals without evaluating their values.
//!
//! Called from:
//! - AST constant folding and codegen default/constant materializers.
//!
//! Key details:
//! - Implicit keys follow the selected PHP profile; source expressions keep their order.
//! - Overflow and nonliteral keys remain unresolved so runtime checks and effects survive.

use crate::parser::ast::{ArrayEntry, Expr, ExprKind};

/// Returns the `ArrayLiteralAssoc` form of a spread-free `ArrayLiteralMixed`, or `None` when
/// `expr` is not one, holds a spread, or has a key the fold cannot classify.
pub(crate) fn fold_spreadless_mixed_array_literal(
    expr: &ExprKind,
    php_version: crate::php_version::PhpVersion,
) -> Option<ExprKind> {
    let ExprKind::ArrayLiteralMixed(entries) = expr else {
        return None;
    };
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
        ExprKind::FloatLiteral(value) => Some(Some(php_float_array_key(*value))),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(Some),
            ExprKind::FloatLiteral(value) => {
                let negated = -*value;
                Some(Some(php_float_array_key(negated)))
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

/// Converts a float key using PHP's signed modulo-2^64 integer conversion.
/// Non-finite values become zero; diagnostics remain the caller's responsibility.
pub(crate) fn php_float_array_key(value: f64) -> i64 {
    if !value.is_finite() { return 0; }
    let word = value.trunc() % 18_446_744_073_709_551_616.0;
    if word >= 9_223_372_036_854_775_808.0 {
        (word - 18_446_744_073_709_551_616.0) as i64
    } else if word < -9_223_372_036_854_775_808.0 {
        (word + 18_446_744_073_709_551_616.0) as i64
    } else {
        word as i64
    }
}
