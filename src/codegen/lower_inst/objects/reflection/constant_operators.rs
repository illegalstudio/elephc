//! Purpose:
//! Evaluates the PHP operators of a Reflection constant expression that need PHP's own value
//! rules: truthiness, loose comparison, logical operators, `(type)` casts and string conversion
//! of floats, booleans and null.
//!
//! Called from:
//! - `super::names_constants::reflection_constant_value()` and
//!   `super::names_constants::reflection_binary_constant_value()`.
//!
//! Key details:
//! - The operands are already evaluated scalars; the operator itself is folded by the shared AST
//!   folder (`crate::optimize::fold_constant_expression`), so `==`, `<`, `<=>`, `.` and casts
//!   answer what the optimizer answers for the same literals.
//! - A shape the folder leaves alone (an array operand, a division by zero, a shift by a negative
//!   count) answers `None`, and the caller reports the expression as unsupported.

use super::{BinOp, Expr, ExprKind, ReflectionConstantValue};
use crate::parser::ast::CastType;
use crate::span::Span;

/// The literal expression of one scalar constant value, or `None` for an array, enum case or
/// undefined constant.
fn value_literal(value: &ReflectionConstantValue, span: Span) -> Option<Expr> {
    let kind = match value {
        ReflectionConstantValue::Int(value) => ExprKind::IntLiteral(*value),
        ReflectionConstantValue::Bool(value) => ExprKind::BoolLiteral(*value),
        ReflectionConstantValue::Float(value) => ExprKind::FloatLiteral(*value),
        ReflectionConstantValue::Str(value) => ExprKind::StringLiteral(value.clone()),
        ReflectionConstantValue::Null => ExprKind::Null,
        _ => return None,
    };
    Some(Expr::new(kind, span))
}

/// The constant value of a folded literal expression, or `None` when the folder left an
/// operator standing.
fn literal_value(expr: &Expr) -> Option<ReflectionConstantValue> {
    Some(match &expr.kind {
        ExprKind::IntLiteral(value) => ReflectionConstantValue::Int(*value),
        ExprKind::BoolLiteral(value) => ReflectionConstantValue::Bool(*value),
        ExprKind::FloatLiteral(value) => ReflectionConstantValue::Float(*value),
        ExprKind::StringLiteral(value) => ReflectionConstantValue::Str(value.clone()),
        ExprKind::Null => ReflectionConstantValue::Null,
        _ => return None,
    })
}

/// php's truthiness of a constant value, or `None` for a value it does not decide (an undefined
/// constant raises when materialized).
pub(super) fn reflection_constant_truthiness(value: &ReflectionConstantValue) -> Option<bool> {
    Some(match value {
        ReflectionConstantValue::Int(value) => *value != 0,
        ReflectionConstantValue::Bool(value) => *value,
        ReflectionConstantValue::Float(value) => *value != 0.0,
        ReflectionConstantValue::Str(value) => !(value.is_empty() || value == "0"),
        ReflectionConstantValue::Null => false,
        ReflectionConstantValue::Array(items) => !items.is_empty(),
        ReflectionConstantValue::AssocArray(entries) => !entries.is_empty(),
        ReflectionConstantValue::EnumCase { .. } => true,
        ReflectionConstantValue::Evaluated { value, .. } => return reflection_constant_truthiness(value),
        ReflectionConstantValue::Undefined(_) => return None,
    })
}

/// php's string conversion of a scalar constant value, or `None` for one this evaluator does not
/// decide: an array, and a float whose `precision=14` form differs from its shortest round-trip
/// form (a value with more than 14 significant digits, or one php prints in exponent notation).
fn scalar_text(value: &ReflectionConstantValue) -> Option<String> {
    Some(match value {
        ReflectionConstantValue::Int(value) => value.to_string(),
        ReflectionConstantValue::Bool(value) => if *value { "1" } else { "" }.to_string(),
        ReflectionConstantValue::Null => String::new(),
        ReflectionConstantValue::Str(value) => value.clone(),
        ReflectionConstantValue::Float(value) => {
            let magnitude = value.abs();
            if !value.is_finite() || (magnitude != 0.0 && !(1e-4..1e15).contains(&magnitude)) {
                return None;
            }
            let text = value.to_string();
            let digits = text.bytes().filter(u8::is_ascii_digit).count();
            let leading_zeros = text
                .trim_start_matches('-')
                .bytes()
                .take_while(|byte| matches!(byte, b'0' | b'.'))
                .filter(|byte| *byte == b'0')
                .count();
            if digits - leading_zeros > 14 {
                return None;
            }
            text
        }
        _ => return None,
    })
}

/// Folds `left op right` over two evaluated operands through the shared AST folder.
///
/// `&&`, `||` and `xor` take the operands' truthiness first, so an array or enum operand answers
/// as php does.
pub(super) fn reflection_operator_constant_value(
    left: &ReflectionConstantValue,
    op: &BinOp,
    right: &ReflectionConstantValue,
    span: Span,
) -> Option<ReflectionConstantValue> {
    if matches!(op, BinOp::Concat) {
        return Some(ReflectionConstantValue::Str(format!(
            "{}{}",
            scalar_text(left)?,
            scalar_text(right)?
        )));
    }
    let (left, right) = if matches!(op, BinOp::And | BinOp::Or | BinOp::Xor) {
        (
            ReflectionConstantValue::Bool(reflection_constant_truthiness(left)?),
            ReflectionConstantValue::Bool(reflection_constant_truthiness(right)?),
        )
    } else {
        (left.clone(), right.clone())
    };
    let expression = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(value_literal(&left, span)?),
            op: op.clone(),
            right: Box::new(value_literal(&right, span)?),
        },
        span,
    );
    literal_value(&crate::optimize::fold_constant_expression(expression))
}

/// Folds `(target) value` over one evaluated scalar through the shared AST folder.
pub(super) fn reflection_cast_constant_value(
    target: &CastType,
    value: &ReflectionConstantValue,
    span: Span,
) -> Option<ReflectionConstantValue> {
    let expression = Expr::new(
        ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(value_literal(value, span)?),
        },
        span,
    );
    literal_value(&crate::optimize::fold_constant_expression(expression))
}
