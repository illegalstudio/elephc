//! Purpose:
//! Evaluates numeric Reflection constant metadata with PHP-equivalent arithmetic.
//!
//! Called from:
//! - `super::names_constants::reflection_binary_constant_value()` after resolving operands.
//!
//! Key details:
//! - Shared folding preserves integer division, overflow promotion, and integer-power rounding.
//! - Reflection also retains non-finite results that ordinary AST folding leaves for runtime.

use super::{BinOp, Expr, ExprKind, ReflectionConstantValue};
use crate::span::Span;

/// Folds resolved numeric operands without duplicating PHP's checked integer arithmetic.
pub(super) fn reflection_numeric_binary_constant_value(
    left: &ReflectionConstantValue,
    op: &BinOp,
    right: &ReflectionConstantValue,
    span: Span,
) -> Option<ReflectionConstantValue> {
    let literal = |value: &ReflectionConstantValue| {
        Some(Expr::new(match value {
            ReflectionConstantValue::Int(value) => ExprKind::IntLiteral(*value),
            ReflectionConstantValue::Float(value) => ExprKind::FloatLiteral(*value),
            _ => return None,
        }, span))
    };
    let expression = Expr::new(ExprKind::BinaryOp {
        left: Box::new(literal(left)?), op: op.clone(), right: Box::new(literal(right)?),
    }, span);
    match crate::optimize::fold_constant_expression(expression).kind {
        ExprKind::IntLiteral(value) => Some(ReflectionConstantValue::Int(value)),
        ExprKind::FloatLiteral(value) => Some(ReflectionConstantValue::Float(value)),
        _ => reflection_float_binary_constant_value(left, op, right),
    }
}

/// Retains supported non-finite results after the shared finite-only folder declines.
fn reflection_float_binary_constant_value(
    left: &ReflectionConstantValue,
    op: &BinOp,
    right: &ReflectionConstantValue,
) -> Option<ReflectionConstantValue> {
    // Integer power keeps the exact exponent parity even when conversion to f64 would lose it.
    let negative_integer_power = matches!((left, op, right),
        (ReflectionConstantValue::Int(base), BinOp::Pow, ReflectionConstantValue::Int(exponent))
            if *base < 0 && *exponent >= 0 && *exponent % 2 != 0);
    let left = reflection_constant_value_as_float(left)?;
    let right = reflection_constant_value_as_float(right)?;
    let value = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div if right != 0.0 => left / right,
        BinOp::Pow if negative_integer_power => -left.abs().powf(right),
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    Some(ReflectionConstantValue::Float(value))
}

/// Returns the float representation of a resolved numeric Reflection value.
fn reflection_constant_value_as_float(value: &ReflectionConstantValue) -> Option<f64> {
    match value {
        ReflectionConstantValue::Int(value) => Some(*value as f64),
        ReflectionConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}
