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
use super::array_key::{php_array_key, PhpArrayKey};
use super::scalar::{
    compare_scalar_exprs, int_literal, loose_eq, numeric_literal, scalar_value, spaceship_scalar,
    strict_eq, ScalarValue,
};

/// Returns the negated literal if the expression is an int or float literal.
///
/// `-PHP_INT_MIN` is not representable as an `i64`; PHP promotes it to the float
/// `9223372036854775808.0` rather than wrapping, so the fold does the same.
pub(super) fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
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
    Some(ExprKind::BoolLiteral(
        !scalar_value(expr)?.diagnostic_free_truthiness()?,
    ))
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
/// Division and power follow PHP's int-preserving rules (see the dedicated helpers).
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
        BinOp::Div => try_fold_int_div(left, right),
        BinOp::Pow => try_fold_int_pow(left, right),
        _ => None,
    }
}

/// Folds integer `/` the way PHP's `div_function` does.
///
/// An exact division stays an integer (`6 / 3` is `int(2)`, not `float(2)`); anything else
/// becomes a float. `PHP_INT_MIN / -1` has no `i64` result, so it also becomes a float
/// instead of overflowing. Division by zero declines so the runtime raises
/// `DivisionByZeroError`.
fn try_fold_int_div(left: i64, right: i64) -> Option<ExprKind> {
    if right == 0 {
        return None;
    }
    match (left.checked_rem(right), left.checked_div(right)) {
        (Some(0), Some(quotient)) => Some(ExprKind::IntLiteral(quotient)),
        _ => Some(ExprKind::FloatLiteral(left as f64 / right as f64)),
    }
}

/// Folds integer `**` the way PHP's `pow_function_base` does.
///
/// A non-negative exponent keeps an integer result when it fits (`2 ** 3` is `int(8)`, not
/// `float(8)`). On overflow PHP does *not* fall back to a single `pow()` call — it multiplies
/// the exact integer accumulated so far by `pow()` of the remaining factor, and the two differ
/// in the last ULP for most inputs, so the square-and-multiply loop is reproduced verbatim.
/// A negative exponent is plain `pow((double) base, (double) exp)` in PHP. A non-finite result
/// declines, leaving the operation on the runtime path.
fn try_fold_int_pow(left: i64, right: i64) -> Option<ExprKind> {
    if right < 0 {
        let result = (left as f64).powf(right as f64);
        return result.is_finite().then_some(ExprKind::FloatLiteral(result));
    }
    if right == 0 {
        return Some(ExprKind::IntLiteral(1));
    }
    if left == 0 {
        return Some(ExprKind::IntLiteral(0));
    }

    let (mut accumulated, mut factor, mut exponent) = (1i64, left, right);
    while exponent >= 1 {
        if exponent % 2 == 1 {
            exponent -= 1;
            match accumulated.checked_mul(factor) {
                Some(product) => accumulated = product,
                None => {
                    let overflowed = accumulated as f64 * factor as f64;
                    return finite_float_literal(overflowed * (factor as f64).powf(exponent as f64));
                }
            }
        } else {
            exponent /= 2;
            match factor.checked_mul(factor) {
                Some(product) => factor = product,
                None => {
                    let overflowed = factor as f64 * factor as f64;
                    return finite_float_literal(accumulated as f64 * overflowed.powf(exponent as f64));
                }
            }
        }
        if exponent == 0 {
            return Some(ExprKind::IntLiteral(accumulated));
        }
    }
    // The loop always returns through the `exponent == 0` check above.
    None
}

/// Wraps a computed float in a literal, declining when it is not finite.
fn finite_float_literal(value: f64) -> Option<ExprKind> {
    value.is_finite().then_some(ExprKind::FloatLiteral(value))
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

/// Returns the integer modulus of two integer literals, or `None` if either operand is not an
/// integer or if the divisor is zero (which raises `DivisionByZeroError` at runtime).
///
/// `PHP_INT_MIN % -1` has no `i64` quotient and traps a native remainder, but PHP defines it
/// as `0`, so the `-1` divisor is answered directly.
fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    match right {
        0 => None,
        -1 => Some(ExprKind::IntLiteral(0)),
        _ => Some(ExprKind::IntLiteral(left % right)),
    }
}

/// Evaluates bitwise AND, OR, XOR, and shift operations on two integer literals.
///
/// Shifts follow PHP's `shift_left_function` / `shift_right_function`: a negative shift
/// declines so the runtime raises `ArithmeticError`, a shift of 64 or more yields `0`
/// (`-1` for a right shift of a negative value), and in-range shifts wrap like the engine's
/// native `<<` / `>>`.
fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    match op {
        BinOp::BitAnd => Some(ExprKind::IntLiteral(left & right)),
        BinOp::BitOr => Some(ExprKind::IntLiteral(left | right)),
        BinOp::BitXor => Some(ExprKind::IntLiteral(left ^ right)),
        BinOp::ShiftLeft => match u32::try_from(right).ok()? {
            shift if shift >= i64::BITS => Some(ExprKind::IntLiteral(0)),
            shift => Some(ExprKind::IntLiteral(left.wrapping_shl(shift))),
        },
        BinOp::ShiftRight => match u32::try_from(right).ok()? {
            shift if shift >= i64::BITS => {
                Some(ExprKind::IntLiteral(if left < 0 { -1 } else { 0 }))
            }
            shift => Some(ExprKind::IntLiteral(left >> shift)),
        },
        _ => None,
    }
}

/// Evaluates logical AND, OR, and XOR on two scalar operands using PHP truthiness rules.
/// Both operands are evaluated (no short-circuit).
fn try_fold_logical_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    let left = left.diagnostic_free_truthiness()?;
    let right = right.diagnostic_free_truthiness()?;
    let result = match op {
        BinOp::And => left && right,
        BinOp::Or => left || right,
        BinOp::Xor => left ^ right,
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
    let condition = scalar_value(condition)?;
    let then_expr = scalar_value(then_expr)?;
    let else_expr = scalar_value(else_expr)?;
    if condition.diagnostic_free_truthiness()? {
        Some(then_expr.into_expr_kind())
    } else {
        Some(else_expr.into_expr_kind())
    }
}

/// Folds a short ternary (`?:) when the value and default are scalar literals.
pub(super) fn try_fold_short_ternary(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    if value.diagnostic_free_truthiness()? {
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
/// The index is normalized through PHP's array-key rules first, so `["a", "b"][true]` and
/// `["a", "b"]["1"]` both select `"b"`. Only succeeds if every element in the array is a
/// scalar literal (required to guarantee the result is foldable) and the index is in range;
/// an out-of-range index declines so the runtime still emits "Undefined array key".
fn try_fold_indexed_array_access(items: &[Expr], index: &Expr) -> Option<ExprKind> {
    let PhpArrayKey::Int(index) = php_array_key(&scalar_value(index)?)? else {
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
/// Keys and the index are normalized through PHP's array-key rules, so `[0 => "a", false =>
/// "b"]` really is a one-entry array and `[0]` selects `"b"`. Duplicate normalized keys are
/// last-wins, matching the order the literal is built in. Declines whenever a key cannot be
/// normalized (a lossy float key) or nothing matches (the runtime owns the warning).
fn try_fold_assoc_array_access(items: &[(Expr, Expr)], index: &Expr) -> Option<ExprKind> {
    let index = php_array_key(&scalar_value(index)?)?;
    let mut selected = None;

    for (key, value) in items {
        let key = php_array_key(&scalar_value(key)?)?;
        let value = scalar_value(value)?;
        if key == index {
            selected = Some(value);
        }
    }

    selected.map(ScalarValue::into_expr_kind)
}
