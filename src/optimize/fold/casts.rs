//! Purpose:
//! Implements constant-folding support for casts expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use super::super::*;
use super::scalar::{scalar_value, ScalarValue};

/// Attempts to constant-fold a cast expression.
 ///
 /// Returns `Some(ExprKind)` with the folded literal if the target and operand are
 /// both scalar and the cast result is unambiguous; `None` otherwise.
 /// Ambiguous cases (float → string, non-finite floats, out-of-range truncations)
 /// return `None` so the cast is evaluated at runtime.
pub(super) fn try_fold_cast(target: &CastType, expr: &Expr) -> Option<ExprKind> {
    let value = scalar_value(expr)?;
    match target {
        CastType::Int => try_fold_cast_int(value),
        CastType::Float => try_fold_cast_float(value),
        CastType::String => try_fold_cast_string(value),
        CastType::Bool => Some(ExprKind::BoolLiteral(value.truthy())),
        CastType::Array => None,
    }
}

/// Folds a scalar `ScalarValue` to an integer literal via `(int)` cast.
 ///
 /// Returns `None` for float values that would truncate to out-of-range or non-finite
 /// values, and for strings that do not parse as i64 or f64 with a representable truncation.
fn try_fold_cast_int(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::IntLiteral(0)),
        ScalarValue::Bool(value) => Some(ExprKind::IntLiteral(i64::from(value))),
        ScalarValue::Int(value) => Some(ExprKind::IntLiteral(value)),
        ScalarValue::Float(value) => truncate_float_to_i64(value).map(ExprKind::IntLiteral),
        ScalarValue::String(value) => parse_string_cast_int(&value).map(ExprKind::IntLiteral),
    }
}

/// Folds a scalar `ScalarValue` to a float literal via `(float)` cast.
 ///
 /// Returns `None` for strings that fail to parse as f64 and contain non-alphabetic
 /// characters, preserving runtime evaluation.
fn try_fold_cast_float(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::FloatLiteral(0.0)),
        ScalarValue::Bool(value) => Some(ExprKind::FloatLiteral(if value { 1.0 } else { 0.0 })),
        ScalarValue::Int(value) => Some(ExprKind::FloatLiteral(value as f64)),
        ScalarValue::Float(value) => Some(ExprKind::FloatLiteral(value)),
        ScalarValue::String(value) => parse_string_cast_float(&value).map(ExprKind::FloatLiteral),
    }
}

/// Folds a scalar `ScalarValue` to a string literal via `(string)` cast.
 ///
 /// Floats are not folded because `(string)1.5` in PHP produces `"1.5"`, not a shortcut.
 /// Null folds to empty string; bool folds to `"1"` or `""`.
fn try_fold_cast_string(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::StringLiteral(String::new())),
        ScalarValue::Bool(value) => Some(ExprKind::StringLiteral(if value {
            "1".to_string()
        } else {
            String::new()
        })),
        ScalarValue::Int(value) => Some(ExprKind::StringLiteral(value.to_string())),
        ScalarValue::Float(_value) => None,
        ScalarValue::String(value) => Some(ExprKind::StringLiteral(value)),
    }
}

/// Truncates an f64 to i64, returning `None` if the value is non-finite or outside
 /// the i64 range. Used by `(int)` cast folding to avoid undefined truncation behavior.
fn truncate_float_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let truncated = value.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return None;
    }
    Some(truncated as i64)
}

/// Parses a string value for `(int)` cast folding.
 ///
 /// Tries i64 parse first, then f64 parse with truncation, then falls back to
 /// all-alphabetic strings (which PHP treats as `0`). Returns `None` for strings
 /// that contain digits or mixed digit/alpha content that fail numeric parsing.
fn parse_string_cast_int(value: &str) -> Option<i64> {
    if let Ok(parsed) = value.parse::<i64>() {
        return Some(parsed);
    }
    if let Ok(parsed) = value.parse::<f64>() {
        return truncate_float_to_i64(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0);
    }
    None
}

/// Parses a string value for `(float)` cast folding.
 ///
 /// Tries f64 parse first; if that fails and all characters are alphabetic, returns `0.0`.
 /// Any other pattern (mixed digits/alpha, punctuation, etc.) returns `None` so the
 /// cast is evaluated at runtime.
fn parse_string_cast_float(value: &str) -> Option<f64> {
    if let Ok(parsed) = value.parse::<f64>() {
        return Some(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0.0);
    }
    None
}
