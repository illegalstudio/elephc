//! Purpose:
//! Implements constant-folding support for ops expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use std::cmp::Ordering;

use super::super::*;
use super::array_key::{array_key, ArrayKey};
use super::scalar::{
    condition_scalar_value, compare_scalar_exprs, int_literal, loose_eq, numeric_literal, scalar_value, spaceship_scalar,
    strict_eq, ScalarValue,
};

/// Folds exact numeric identity while retaining any source conversion, warning or domain guard.
pub(super) fn try_fold_unary_plus(expr: &Expr) -> Option<ExprKind> {
    if crate::semantics::numeric::operator_rule(super::super::semantic_context::active_profile(),
        super::super::semantic_context::active_source_mode(), elephc_semantics::numeric_domains::NumericOperator::Unary).is_some()
        && !matches!(expr.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)) { return None; }
    crate::semantics::numeric::literal_unary_plus(expr).map(|value| value.kind)
}

/// Returns the negated literal if the expression is an int or float literal.
///
/// `-PHP_INT_MIN` is not representable as an `i64`; PHP promotes it to the float
/// `9223372036854775808.0` rather than wrapping, so the fold does the same.
pub(super) fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
    if super::super::semantic_context::enabled(elephc_semantics::StrictFeature::Overflow)
        && matches!(expr.kind, ExprKind::IntLiteral(i64::MIN)) { return None; }
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(match value.checked_neg() {
            Some(negated) => ExprKind::IntLiteral(negated),
            None => ExprKind::FloatLiteral(-(*value as f64)),
        }),
        ExprKind::FloatLiteral(value) => Some(ExprKind::FloatLiteral(-value)),
        _ => None,
    }
}

/// Returns the logical negation of a scalar expression as a BoolLiteral, or `None` if the operand is not a scalar.
pub(super) fn try_fold_not(expr: &Expr) -> Option<ExprKind> {
    Some(ExprKind::BoolLiteral(!condition_scalar_value(expr)?.truthy()))
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

/// Adapts numeric literals to the shared side-effect-free fold without changing source eligibility.
fn try_fold_numeric_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    use elephc_semantics::{numeric::IntegerOp, numeric_unary::NumericValue};
    let operation = match op {
        BinOp::Add => IntegerOp::Add, BinOp::Sub => IntegerOp::Sub,
        BinOp::Mul => IntegerOp::Mul, BinOp::Div => IntegerOp::Div,
        BinOp::Pow => IntegerOp::Pow, _ => return None,
    };
    let literal = |value: &Expr| int_literal(value).map(NumericValue::Int)
        .or_else(|| numeric_literal(value).map(NumericValue::Float));
    let profile = crate::semantics::numeric::operation_policy(
        super::super::semantic_context::active_profile(),
        super::super::semantic_context::active_source_mode(),
    ).requirements();
    elephc_semantics::constant_numeric::fold(profile, operation, literal(left)?, literal(right)?)
        .map(|value| match value {
            NumericValue::Int(value) => ExprKind::IntLiteral(value),
            NumericValue::Float(value) => ExprKind::FloatLiteral(value),
        })
}

/// Returns the integer modulus of two integer literals, or `None` if either operand is not an
/// integer or if the divisor is zero (which raises `DivisionByZeroError` at runtime).
///
/// `PHP_INT_MIN % -1` has no `i64` quotient and traps a native remainder, but PHP defines it
/// as `0`, so the `-1` divisor is answered directly.
fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    use elephc_semantics::{numeric::IntegerOp, numeric_unary::NumericValue};
    let value = elephc_semantics::constant_numeric::fold(super::super::semantic_context::active_profile(),
        IntegerOp::Mod, NumericValue::Int(int_literal(left)?), NumericValue::Int(int_literal(right)?))?;
    let NumericValue::Int(value) = value else { return None; };
    Some(ExprKind::IntLiteral(value))
}

/// Evaluates bitwise AND, OR, XOR, and shift operations on two integer literals.
///
/// Shifts follow PHP's `shift_left_function` / `shift_right_function`: a negative shift
/// declines so the runtime raises `ArithmeticError`, a shift of 64 or more yields `0`
/// (`-1` for a right shift of a negative value), and in-range shifts wrap like the engine's
/// native `<<` / `>>`.
fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    let operation = crate::semantics::numeric::integer_bit_op(op)?;
    elephc_semantics::numeric_domains::integer_bitwise(operation, left, right).ok().map(ExprKind::IntLiteral)
}

/// Evaluates logical AND, OR, and XOR on two scalar operands using PHP truthiness rules.
/// Both operands are evaluated (no short-circuit).
fn try_fold_logical_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let left = condition_scalar_value(left)?;
    let right = condition_scalar_value(right)?;
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
///
/// PHP implements `a > b` as `b < a` and `a >= b` as `b <= a`, so the relational arms swap
/// the operands rather than inverting the ordering. That is what keeps every NAN comparison
/// false: `zend_compare` answers `1` for any NAN pair in either direction.
fn try_fold_compare_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    if super::super::semantic_context::enabled(elephc_semantics::StrictFeature::Comparisons) {
        if matches!(op, BinOp::Eq | BinOp::NotEq) { return None; }
        if crate::semantics::comparisons::is_ordering(op)
            && std::mem::discriminant(&scalar_value(left)?) != std::mem::discriminant(&scalar_value(right)?)
        { return None; }
    }
    match op {
        BinOp::Eq => Some(ExprKind::BoolLiteral(loose_eq(left, right)?)),
        BinOp::NotEq => Some(ExprKind::BoolLiteral(!loose_eq(left, right)?)),
        BinOp::StrictEq => Some(ExprKind::BoolLiteral(strict_eq(left, right)?)),
        BinOp::StrictNotEq => Some(ExprKind::BoolLiteral(!strict_eq(left, right)?)),
        BinOp::Lt => Some(ExprKind::BoolLiteral(
            compare_scalar_exprs(left, right)? == Ordering::Less,
        )),
        BinOp::Gt => Some(ExprKind::BoolLiteral(
            compare_scalar_exprs(right, left)? == Ordering::Less,
        )),
        BinOp::LtEq => Some(ExprKind::BoolLiteral(
            compare_scalar_exprs(left, right)? != Ordering::Greater,
        )),
        BinOp::GtEq => Some(ExprKind::BoolLiteral(
            compare_scalar_exprs(right, left)? != Ordering::Greater,
        )),
        BinOp::Spaceship => Some(ExprKind::IntLiteral(spaceship_scalar(left, right)?)),
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
    let condition = condition_scalar_value(condition)?;
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
    let value = condition_scalar_value(value)?;
    if value.truthy() {
        Some(value.into_expr_kind())
    } else {
        Some(scalar_value(default)?.into_expr_kind())
    }
}

/// Folds an array literal access when the array and index are both scalar literals with a known result.
pub(in crate::optimize) fn try_fold_array_access(array: &Expr, index: &Expr) -> Option<ExprKind> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => try_fold_indexed_array_access(items, index),
        ExprKind::ArrayLiteralAssoc(items) => try_fold_assoc_array_access(items, index),
        _ => None,
    }
}

/// Returns the array element at a given index when all array elements and the index are scalar
/// literals.
///
/// The index follows the selected key policy, so exact string keys never select packed slots.
/// Only succeeds if every element in the array is a
/// scalar literal (required to guarantee the result is foldable) and the index is in range;
/// an out-of-range index declines so the runtime still emits "Undefined array key".
fn try_fold_indexed_array_access(items: &[Expr], index: &Expr) -> Option<ExprKind> {
    let ArrayKey::Int(index) = array_key(&scalar_value(index)?)? else {
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

/// Returns the value associated with a matching key in an associative array literal when all
/// keys and values are scalar literals.
///
/// Keys and the index use the selected typed identity, with last-wins replacement only for
/// matching keys. Declines for rejected exact-key domains, lossy PHP float keys or a missing key,
/// preserving the original domain check or runtime warning.
fn try_fold_assoc_array_access(items: &[(Expr, Expr)], index: &Expr) -> Option<ExprKind> {
    let index = array_key(&scalar_value(index)?)?;
    let mut selected = None;

    for (key, value) in items {
        let key = array_key(&scalar_value(key)?)?;
        let value = scalar_value(value)?;
        if key == index {
            selected = Some(value);
        }
    }

    selected.map(ScalarValue::into_expr_kind)
}
