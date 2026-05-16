//! Purpose:
//! Implements constant-folding support for scalar expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use super::super::*;

pub(in crate::optimize) fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

pub(in crate::optimize) fn numeric_literal(expr: &Expr) -> Option<f64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value as f64),
        ExprKind::FloatLiteral(value) => Some(value),
        _ => None,
    }
}

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

pub(in crate::optimize) fn assigned_scalar_value(expr: &Expr) -> Option<ScalarValue> {
    scalar_value(expr).or_else(|| match &expr.kind {
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_value = assigned_scalar_value(then_expr)?;
            let else_value = assigned_scalar_value(else_expr)?;
            (then_value == else_value).then_some(then_value)
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
                .all(|(_, value)| assigned_scalar_value(value) == Some(default_value.clone()))
                .then_some(default_value)
        }
        _ => None,
    })
}

pub(in crate::optimize) fn strict_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    Some(left == right)
}

pub(in crate::optimize) fn loose_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    match (&left, &right) {
        (ScalarValue::Bool(left), right) => Some(*left == right.truthy()),
        (left, ScalarValue::Bool(right)) => Some(left.truthy() == *right),
        (ScalarValue::Null, ScalarValue::Null) => Some(true),
        (ScalarValue::Null, ScalarValue::String(right)) => Some(right.is_empty()),
        (ScalarValue::String(left), ScalarValue::Null) => Some(left.is_empty()),
        (ScalarValue::Null, ScalarValue::Int(right)) => Some(*right == 0),
        (ScalarValue::Int(left), ScalarValue::Null) => Some(*left == 0),
        (ScalarValue::Null, ScalarValue::Float(right)) => Some(*right == 0.0),
        (ScalarValue::Float(left), ScalarValue::Null) => Some(*left == 0.0),
        (ScalarValue::String(left), ScalarValue::String(right)) => {
            match (php_numeric_string(left), php_numeric_string(right)) {
                (Some(left), Some(right)) => Some(left == right),
                _ => Some(left == right),
            }
        }
        (ScalarValue::Int(left), ScalarValue::Int(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Float(right)) => Some(*left as f64 == *right),
        (ScalarValue::Float(left), ScalarValue::Int(right)) => Some(*left == *right as f64),
        (ScalarValue::Int(left), ScalarValue::String(right)) => {
            php_numeric_string(right).map(|right| *left as f64 == right).or(Some(false))
        }
        (ScalarValue::String(left), ScalarValue::Int(right)) => {
            php_numeric_string(left).map(|left| left == *right as f64).or(Some(false))
        }
        (ScalarValue::Float(left), ScalarValue::String(right)) => {
            php_numeric_string(right).map(|right| *left == right).or(Some(false))
        }
        (ScalarValue::String(left), ScalarValue::Float(right)) => {
            php_numeric_string(left).map(|left| left == *right).or(Some(false))
        }
    }
}

fn php_numeric_string(value: &str) -> Option<f64> {
    let trimmed = value.trim_matches(|c: char| c.is_ascii_whitespace());
    if trimmed.is_empty() {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    if matches!(bytes[idx], b'+' | b'-') {
        idx += 1;
        if idx == bytes.len() {
            return None;
        }
    }

    let mut digits = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
        digits += 1;
    }

    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }

    if idx < bytes.len() && matches!(bytes[idx], b'e' | b'E') {
        idx += 1;
        if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
            idx += 1;
        }
        let exp_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == exp_start {
            return None;
        }
    }

    if idx != bytes.len() {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|value| value.is_finite())
}

pub(in crate::optimize) fn compare_numeric(
    left: &Expr,
    right: &Expr,
    cmp: impl FnOnce(f64, f64) -> bool,
) -> Option<bool> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(cmp(left, right))
}

pub(in crate::optimize) fn spaceship_numeric(left: &Expr, right: &Expr) -> Option<i64> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::optimize) enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    pub(in crate::optimize) fn truthy(&self) -> bool {
        match self {
            ScalarValue::Null => false,
            ScalarValue::Bool(value) => *value,
            ScalarValue::Int(value) => *value != 0,
            ScalarValue::Float(value) => *value != 0.0,
            ScalarValue::String(value) => !value.is_empty() && value != "0",
        }
    }

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
