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
use super::compare::{php_numeric_prefix, PhpNumeric};
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
        // A NAN is truthy, but PHP 8.5 also REPORTS the coercion
        // (`unexpected NAN value was coerced to bool`), and a folded `BoolLiteral` would
        // swallow that warning — `var_dump((bool)NAN)` warns in php-src even though the
        // operand is a compile-time constant. Declining the fold sends the value through the
        // runtime truthiness path, which owns the diagnostic. This costs one compare for a
        // literal NAN and nothing at all for every other float, and it matches how
        // `try_fold_cast_int`/`try_fold_cast_string` already decline non-finite operands.
        CastType::Bool if value.is_nan_float() => None,
        CastType::Bool => Some(ExprKind::BoolLiteral(value.truthy())),
        CastType::Array | CastType::Object => None,
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

/// Parses a string value for `(int)` cast folding, reproducing PHP's `zval_get_long()`.
 ///
 /// The string's leading numeric run decides the result: an integer run that fits `i64` is
 /// used directly, anything else goes through `zend_dval_to_lval_cap` (NAN/INF become `0`,
 /// out-of-range floats saturate). A string with no numeric prefix at all is `0`, so
 /// `"abc"`, `"0x1A"` and `"INF"` all fold to `0` rather than being parsed as Rust numbers.
fn parse_string_cast_int(value: &str) -> Option<i64> {
    Some(match php_numeric_prefix(value) {
        Some(PhpNumeric::Int(parsed)) => parsed,
        Some(PhpNumeric::Float { value, .. }) => cap_float_to_i64(value),
        None => 0,
    })
}

/// Parses a string value for `(float)` cast folding, reproducing PHP's `zend_strtod()`.
 ///
 /// Only the leading numeric run counts, and PHP's grammar has no `INF`, `NAN`, hexadecimal
 /// or underscore forms — unlike Rust's `str::parse::<f64>()`, which accepts `"inf"` and
 /// `"nan"`. A string with no numeric prefix is `0.0`.
fn parse_string_cast_float(value: &str) -> Option<f64> {
    Some(match php_numeric_prefix(value) {
        Some(PhpNumeric::Int(parsed)) => parsed as f64,
        Some(PhpNumeric::Float { value, .. }) => value,
        None => 0.0,
    })
}

/// Saturating float-to-int conversion, PHP's `zend_dval_to_lval_cap()`.
 ///
 /// Non-finite values become `0`; values outside the `i64` range clamp to `PHP_INT_MAX` /
 /// `PHP_INT_MIN`; everything else truncates toward zero. Used only by the string `(int)`
 /// cast, which is the one PHP path that saturates instead of wrapping.
fn cap_float_to_i64(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    // `i64::MAX as f64` rounds up to 2^63, so the upper bound is exclusive.
    if value >= -(i64::MIN as f64) {
        return i64::MAX;
    }
    if value < i64::MIN as f64 {
        return i64::MIN;
    }
    value as i64
}
