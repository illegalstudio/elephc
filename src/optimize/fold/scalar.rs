//! Purpose:
//! Implements constant-folding support for scalar expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use std::cmp::Ordering;

use super::super::*;
use super::compare::{compare_scalars, loose_eq_values};

/// Extracts an i64 from an integer literal expression.
pub(in crate::optimize) fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

/// Extracts an f64 from an integer or float literal expression.
pub(in crate::optimize) fn numeric_literal(expr: &Expr) -> Option<f64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value as f64),
        ExprKind::FloatLiteral(value) => Some(value),
        _ => None,
    }
}

/// Extracts a ScalarValue from a scalar literal expression (Null, Bool, Int, Float, String).
pub(in crate::optimize) fn scalar_value(expr: &Expr) -> Option<ScalarValue> {
    match &expr.kind {
        ExprKind::Null => Some(ScalarValue::Null),
        ExprKind::BoolLiteral(value) => Some(ScalarValue::Bool(*value)),
        ExprKind::IntLiteral(value) => Some(ScalarValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(ScalarValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ScalarValue::String(value.clone())),
        _ => None,
    }
}

/// Extracts a ScalarValue from an expression, unwrapping ternary/match branches when both arms yield the same value.
///
/// Returns `None` if the expression is not a scalar literal or a ternary/match whose
/// arms are all identical scalars. Used by DCE to determine whether an assignment
/// target has a known compile-time value.
pub(in crate::optimize) fn assigned_scalar_value(expr: &Expr) -> Option<ScalarValue> {
    scalar_value(expr).or_else(|| match &expr.kind {
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_value = assigned_scalar_value(then_expr)?;
            let else_value = assigned_scalar_value(else_expr)?;
            then_value.same_constant(&else_value).then_some(then_value)
        }
        ExprKind::ShortTernary { value, default } => {
            let value = assigned_scalar_value(value)?;
            if value.truthy() {
                Some(value)
            } else {
                assigned_scalar_value(default)
            }
        }
        ExprKind::Match { arms, default, .. } => {
            let default = default.as_ref()?;
            let default_value = assigned_scalar_value(default)?;
            arms.iter()
                .all(|(_, value)| {
                    assigned_scalar_value(value)
                        .is_some_and(|value| value.same_constant(&default_value))
                })
                .then_some(default_value)
        }
        _ => None,
    })
}

/// Upper bound on array-literal fact size: facts are cloned into propagation
/// environments and across control-flow merges, so oversized literals simply
/// carry no fact.
pub(in crate::optimize) const MAX_ARRAY_FACT_ELEMENTS: usize = 64;

/// Extracts a qualifying array-literal fact from an assignment RHS: an indexed
/// or associative literal whose keys and values are all scalar literals, at
/// most `MAX_ARRAY_FACT_ELEMENTS` entries. The returned clone feeds
/// `try_fold_array_access` at constant-index reads of the assigned variable.
pub(in crate::optimize) fn assigned_array_fact(expr: &Expr) -> Option<Expr> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items)
            if items.len() <= MAX_ARRAY_FACT_ELEMENTS
                && items.iter().all(|item| scalar_value(item).is_some()) =>
        {
            Some(expr.clone())
        }
        ExprKind::ArrayLiteralAssoc(items)
            if items.len() <= MAX_ARRAY_FACT_ELEMENTS
                && items
                    .iter()
                    .all(|(key, value)| scalar_value(key).is_some() && scalar_value(value).is_some()) =>
        {
            Some(expr.clone())
        }
        _ => None,
    }
}

/// Returns `Some(true)` if two scalar expressions are strictly equal (===), `Some(false)` if not,
/// or `None` if either operand is not a scalar literal.
pub(in crate::optimize) fn strict_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    Some(left == right)
}

/// Returns `Some(true)` if two scalar expressions are loosely equal (==) per PHP coercion rules,
/// `Some(false)` if not, or `None` if either operand is not a scalar literal or the pair has no
/// compile-time answer (a float against a non-numeric string).
pub(in crate::optimize) fn loose_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    loose_eq_values(&left, &right)
}

/// Returns PHP's `zend_compare()` ordering for two scalar literal expressions.
///
/// Returns `None` when either operand is not a scalar literal, or when the pair has no
/// compile-time answer. Relational folding must pass the operands in the order the engine
/// uses (`a > b` is `compare(b, a) == Less`) so NAN keeps PHP's behavior.
pub(in crate::optimize) fn compare_scalar_exprs(left: &Expr, right: &Expr) -> Option<Ordering> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    compare_scalars(&left, &right)
}

/// Returns the result of the spaceship operator (`<=>`) on two scalar literals as -1, 0, or 1,
/// or `None` when the comparison has no compile-time answer.
pub(in crate::optimize) fn spaceship_scalar(left: &Expr, right: &Expr) -> Option<i64> {
    Some(match compare_scalar_exprs(left, right)? {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

/// Represents a scalar value extracted from a literal expression.
/// Used during constant folding to compare, coerce, and reconstruct literal expressions.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::optimize) enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    /// Returns whether this scalar value is truthy per PHP rules.
    pub(in crate::optimize) fn truthy(&self) -> bool {
        match self {
            ScalarValue::Null => false,
            ScalarValue::Bool(value) => *value,
            ScalarValue::Int(value) => *value != 0,
            ScalarValue::Float(value) => *value != 0.0,
            ScalarValue::String(value) => !value.is_empty() && value != "0",
        }
    }

    /// Returns whether two scalar values denote the *same constant*, i.e. whether one can be
    /// substituted for the other without changing any observable byte of the program.
    ///
    /// This is deliberately stricter than `PartialEq`: floats are compared by bit pattern, so
    /// `0.0` and `-0.0` stay distinct (`echo -0.0` prints `-0`) and two NANs with the same
    /// payload merge. Use it for constant identity — merging ternary/match arms into one
    /// propagated fact — never for PHP's `==` or `===`, which are value comparisons.
    pub(in crate::optimize) fn same_constant(&self, other: &Self) -> bool {
        match (self, other) {
            (ScalarValue::Float(left), ScalarValue::Float(right)) => {
                left.to_bits() == right.to_bits()
            }
            (left, right) => left == right,
        }
    }

    /// Returns whether this scalar value is a floating-point NAN.
    ///
    /// Folding a NAN to bool is value-correct (`truthy()` already answers `true`, since
    /// `NAN != 0.0`) but WARNING-incorrect under PHP 8.5, which reports every NAN-to-bool
    /// coercion. `try_fold_cast` uses this to keep such a cast on the runtime path.
    pub(in crate::optimize) fn is_nan_float(&self) -> bool {
        matches!(self, ScalarValue::Float(value) if value.is_nan())
    }

    /// Returns PHP truthiness only when folding cannot suppress a diagnostic.
    ///
    /// PHP 8.5 reports every NAN-to-bool coercion. Optimizer callers use this gate before
    /// replacing control flow or a logical expression with a literal, so a program whose
    /// only observable effect is that warning keeps it.
    pub(in crate::optimize) fn diagnostic_free_truthiness(&self) -> Option<bool> {
        (!self.is_nan_float()).then(|| self.truthy())
    }

    /// Converts this scalar value back into the equivalent `ExprKind` literal node.
    pub(in crate::optimize) fn into_expr_kind(self) -> ExprKind {
        match self {
            ScalarValue::Null => ExprKind::Null,
            ScalarValue::Bool(value) => ExprKind::BoolLiteral(value),
            ScalarValue::Int(value) => ExprKind::IntLiteral(value),
            ScalarValue::Float(value) => ExprKind::FloatLiteral(value),
            ScalarValue::String(value) => ExprKind::StringLiteral(value),
        }
    }
}
