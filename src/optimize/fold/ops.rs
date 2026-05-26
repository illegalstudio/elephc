//! Purpose:
//! Implements constant-folding support for ops expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use super::super::*;
use super::scalar::{
    compare_numeric, int_literal, loose_eq, numeric_literal, scalar_value, spaceship_numeric,
    strict_eq, ScalarValue,
};

/// Returns the negated literal if the expression is an int or float literal that can be negated without overflow.
pub(super) fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => value.checked_neg().map(ExprKind::IntLiteral),
        ExprKind::FloatLiteral(value) => Some(ExprKind::FloatLiteral(-value)),
        _ => None,
    }
}

/// Returns the logical negation of a scalar expression as a BoolLiteral, or `None` if the operand is not a scalar.
pub(super) fn try_fold_not(expr: &Expr) -> Option<ExprKind> {
    Some(ExprKind::BoolLiteral(!scalar_value(expr)?.truthy()))
}

/// Returns the bitwise NOT of an integer literal, or `None` if the operand is not an integer literal.
pub(super) fn try_fold_bit_not(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ExprKind::IntLiteral(!value)),
        _ => None,
    }
}

/// Attempts to constant-fold a binary operator with two scalar operands.
///
/// Returns the folded `ExprKind` literal when both operands are scalar literals and
/// the operation has an unambiguous PHP-equivalent result; `None` otherwise.
/// Division by zero and overflow cases return `None` to preserve PHP runtime behavior.
pub(super) fn try_fold_binary_op(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Concat => try_fold_concat(left, right),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            try_fold_numeric_binop(op, left, right)
        }
        BinOp::Mod => try_fold_int_mod(left, right),
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            try_fold_bitwise_binop(op, left, right)
        }
        BinOp::And | BinOp::Or | BinOp::Xor => try_fold_logical_binop(op, left, right),
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::StrictEq
        | BinOp::StrictNotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq
        | BinOp::Spaceship => try_fold_compare_binop(op, left, right),
        _ => None,
    }
}

/// Returns the concatenation of two string literals as a `StringLiteral`, or `None` if either operand is not a string literal.
fn try_fold_concat(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let ExprKind::StringLiteral(left) = &left.kind else {
        return None;
    };
    let ExprKind::StringLiteral(right) = &right.kind else {
        return None;
    };
    Some(ExprKind::StringLiteral(format!("{left}{right}")))
}

/// Evaluates a numeric binary operator when at least one operand is a float or when integer overflow occurs.
/// Falls back to float result for overflow cases (add, sub, mul) to match PHP behavior.
/// Returns `None` for division by zero or non-finite results.
fn try_fold_numeric_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    if let (Some(left), Some(right)) = (int_literal(left), int_literal(right)) {
        return try_fold_int_numeric_binop(op, left, right);
    }

    let (left, right) = (numeric_literal(left)?, numeric_literal(right)?);
    if matches!(op, BinOp::Div) && right == 0.0 {
        return None;
    }
    let result = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    if result.is_finite() {
        Some(ExprKind::FloatLiteral(result))
    } else {
        None
    }
}

/// Evaluates a numeric binary operator with two integer operands using checked arithmetic.
/// On overflow for add/sub/mul, delegates to `fold_int_overflow_to_float` to produce a float result.
/// Division always returns a float; power returns a float if finite.
fn try_fold_int_numeric_binop(op: &BinOp, left: i64, right: i64) -> Option<ExprKind> {
    match op {
        BinOp::Add => left
            .checked_add(right)
            .map(ExprKind::IntLiteral)
            .or_else(|| fold_int_overflow_to_float(op, left, right)),
        BinOp::Sub => left
            .checked_sub(right)
            .map(ExprKind::IntLiteral)
            .or_else(|| fold_int_overflow_to_float(op, left, right)),
        BinOp::Mul => left
            .checked_mul(right)
            .map(ExprKind::IntLiteral)
            .or_else(|| fold_int_overflow_to_float(op, left, right)),
        BinOp::Div => {
            if right == 0 {
                None
            } else {
                Some(ExprKind::FloatLiteral(left as f64 / right as f64))
            }
        }
        BinOp::Pow => {
            let result = (left as f64).powf(right as f64);
            if result.is_finite() {
                Some(ExprKind::FloatLiteral(result))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Converts overflowed integer add/sub/mul operations to float to match PHP's numeric coercion.
/// Returns `None` if the resulting float is non-finite.
fn fold_int_overflow_to_float(op: &BinOp, left: i64, right: i64) -> Option<ExprKind> {
    let result = match op {
        BinOp::Add => left as f64 + right as f64,
        BinOp::Sub => left as f64 - right as f64,
        BinOp::Mul => left as f64 * right as f64,
        _ => return None,
    };
    result.is_finite().then_some(ExprKind::FloatLiteral(result))
}

/// Returns the integer modulus of two integer literals, or `None` if either operand is not an integer or if the divisor is zero.
fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    if right == 0 {
        None
    } else {
        Some(ExprKind::IntLiteral(left % right))
    }
}

/// Evaluates bitwise AND, OR, XOR, and shift operations on two integer literals.
/// Shift amounts must fit in a `u32`; returns `None` for invalid shift amounts.
fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    match op {
        BinOp::BitAnd => Some(ExprKind::IntLiteral(left & right)),
        BinOp::BitOr => Some(ExprKind::IntLiteral(left | right)),
        BinOp::BitXor => Some(ExprKind::IntLiteral(left ^ right)),
        BinOp::ShiftLeft => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shl(shift).map(ExprKind::IntLiteral)
        }
        BinOp::ShiftRight => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shr(shift).map(ExprKind::IntLiteral)
        }
        _ => None,
    }
}

/// Evaluates logical AND, OR, and XOR on two scalar operands using PHP truthiness rules.
/// Both operands are evaluated (no short-circuit).
fn try_fold_logical_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    let result = match op {
        BinOp::And => left.truthy() && right.truthy(),
        BinOp::Or => left.truthy() || right.truthy(),
        BinOp::Xor => left.truthy() ^ right.truthy(),
        _ => return None,
    };
    Some(ExprKind::BoolLiteral(result))
}

/// Evaluates comparison operators (equality, relational, spaceship) on two scalar operands.
/// Returns `None` if operands cannot be compared.
fn try_fold_compare_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Eq => Some(ExprKind::BoolLiteral(loose_eq(left, right)?)),
        BinOp::NotEq => Some(ExprKind::BoolLiteral(!loose_eq(left, right)?)),
        BinOp::StrictEq => Some(ExprKind::BoolLiteral(strict_eq(left, right)?)),
        BinOp::StrictNotEq => Some(ExprKind::BoolLiteral(!strict_eq(left, right)?)),
        BinOp::Lt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l < r)?)),
        BinOp::Gt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l > r)?)),
        BinOp::LtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l <= r)?)),
        BinOp::GtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l >= r)?)),
        BinOp::Spaceship => Some(ExprKind::IntLiteral(spaceship_numeric(left, right)?)),
        _ => None,
    }
}

/// Folds the null-coalescing operator when the value is `Null`, replacing it with the default.
pub(super) fn try_fold_null_coalesce(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    let default = scalar_value(default)?;
    if matches!(value, ScalarValue::Null) {
        Some(default.into_expr_kind())
    } else {
        Some(value.into_expr_kind())
    }
}

/// Folds a ternary expression when the condition and both branches are scalar literals.
pub(super) fn try_fold_ternary(
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
) -> Option<ExprKind> {
    let condition = scalar_value(condition)?;
    let then_expr = scalar_value(then_expr)?;
    let else_expr = scalar_value(else_expr)?;
    if condition.truthy() {
        Some(then_expr.into_expr_kind())
    } else {
        Some(else_expr.into_expr_kind())
    }
}

/// Folds a short ternary (`?:) when the value and default are scalar literals.
pub(super) fn try_fold_short_ternary(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    if value.truthy() {
        Some(value.into_expr_kind())
    } else {
        Some(scalar_value(default)?.into_expr_kind())
    }
}

/// Folds an array literal access when the array and index are both scalar literals with a known result.
pub(super) fn try_fold_array_access(array: &Expr, index: &Expr) -> Option<ExprKind> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => try_fold_indexed_array_access(items, index),
        ExprKind::ArrayLiteralAssoc(items) => try_fold_assoc_array_access(items, index),
        _ => None,
    }
}

/// Returns the array element at a given numeric index when all array elements and the index are scalar literals.
/// Only succeeds if every element in the array is a scalar literal (required to guarantee the result is foldable).
fn try_fold_indexed_array_access(items: &[Expr], index: &Expr) -> Option<ExprKind> {
    let ScalarValue::Int(index) = scalar_value(index)? else {
        return None;
    };
    let index = usize::try_from(index).ok()?;
    let value = items.get(index)?;

    items
        .iter()
        .all(|item| scalar_value(item).is_some())
        .then(|| scalar_value(value).map(ScalarValue::into_expr_kind))
        .flatten()
}

/// Returns the value associated with a matching key in an associative array literal when all keys and values are scalar literals.
/// Uses scalar equality for key matching; returns the first matching value if multiple keys compare equal.
fn try_fold_assoc_array_access(items: &[(Expr, Expr)], index: &Expr) -> Option<ExprKind> {
    let index = scalar_value(index)?;
    let mut selected = None;

    for (key, value) in items {
        let key = scalar_value(key)?;
        let value = scalar_value(value)?;
        if key == index {
            selected = Some(value);
        }
    }

    selected.map(ScalarValue::into_expr_kind)
}
