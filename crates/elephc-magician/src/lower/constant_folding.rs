//! Purpose:
//! Folds the literal operands of a `const` initializer the way php-src's
//! `zend_eval_const_expr` does before it validates the expression, so the validator knows
//! which branch of a ternary, `?:`, `??`, `&&` or `||` PHP discards.
//!
//! Called from:
//! - `crate::lower::constant_expressions`.
//!
//! Key details:
//! - Folding is conservative: it returns `None` for anything whose PHP result it does not
//!   reproduce exactly (constants, floats rendered as strings, numeric-string arithmetic,
//!   loose comparisons across types, operations PHP leaves to run time such as a division by
//!   zero). `None` keeps both branches checked, which can only be stricter than PHP.
//! - Of the magic constants only `__LINE__` is folded. PHP folds `__CLASS__`, `__FUNCTION__`,
//!   `__METHOD__` and `__TRAIT__` to `""` in a `const` initializer, but a nested `eval()` run
//!   from an eval-declared function or method still reads its caller's names at run time, so
//!   folding them here could validate a branch the statement then does not take.

use crate::eval_ir::{EvalBinOp, EvalCastType, EvalConst, EvalExpr, EvalMagicConst, EvalUnaryOp};

/// A value php-src computes while compiling a constant expression.
#[derive(Clone, PartialEq)]
pub(super) enum Folded {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    /// Only whether the array is empty decides truthiness, and only that is retained.
    Array { empty: bool },
}

impl Folded {
    /// Returns PHP's boolean conversion of the folded value.
    pub(super) fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !(value.is_empty() || value.as_slice() == b"0"),
            Self::Array { empty } => !empty,
        }
    }

    /// Returns the string conversion PHP applies to a concatenation operand, when exact.
    fn string_bytes(&self) -> Option<Vec<u8>> {
        Some(match self {
            Self::Null | Self::Bool(false) => Vec::new(),
            Self::Bool(true) => b"1".to_vec(),
            Self::Int(value) => value.to_string().into_bytes(),
            Self::String(value) => value.clone(),
            Self::Float(_) | Self::Array { .. } => return None,
        })
    }
}

/// Folds `expr` to the value php-src computes at compile time, or `None` when it would not.
pub(super) fn fold(expr: &EvalExpr) -> Option<Folded> {
    Some(match expr {
        EvalExpr::Const(constant) => match constant {
            EvalConst::Null => Folded::Null,
            EvalConst::Bool(value) => Folded::Bool(*value),
            EvalConst::Int(value) => Folded::Int(*value),
            EvalConst::Float(value) => Folded::Float(*value),
            EvalConst::String(value) => Folded::String(value.as_bytes().to_vec()),
            EvalConst::Bytes(value) => Folded::String(value.clone()),
        },
        EvalExpr::Magic(EvalMagicConst::Line(line)) => Folded::Int(*line),
        EvalExpr::Array(elements) => Folded::Array { empty: elements.is_empty() },
        EvalExpr::Unary { op, expr } => fold_unary(*op, fold(expr)?)?,
        EvalExpr::Cast { target, expr } => fold_cast(*target, fold(expr)?)?,
        EvalExpr::Binary { op: op @ (EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr), left, right } => {
            let left = fold(left)?.truthy();
            if left == (*op == EvalBinOp::LogicalOr) {
                Folded::Bool(left)
            } else {
                Folded::Bool(fold(right)?.truthy())
            }
        }
        EvalExpr::Binary { op, left, right } => fold_binary(*op, fold(left)?, fold(right)?)?,
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            let condition = fold(condition)?;
            match (condition.truthy(), then_branch) {
                (true, Some(then_branch)) => fold(then_branch)?,
                (true, None) => condition,
                (false, _) => fold(else_branch)?,
            }
        }
        EvalExpr::NullCoalesce { value, default } => match fold(value)? {
            Folded::Null => fold(default)?,
            value => value,
        },
        _ => return None,
    })
}

/// Folds one unary operator over an exactly representable operand.
fn fold_unary(op: EvalUnaryOp, value: Folded) -> Option<Folded> {
    Some(match (op, value) {
        (EvalUnaryOp::LogicalNot, value) => Folded::Bool(!value.truthy()),
        (EvalUnaryOp::Plus, value @ (Folded::Int(_) | Folded::Float(_))) => value,
        (EvalUnaryOp::Negate, Folded::Int(value)) => {
            value.checked_neg().map_or(Folded::Float(-(value as f64)), Folded::Int)
        }
        (EvalUnaryOp::Negate, Folded::Float(value)) => Folded::Float(-value),
        (EvalUnaryOp::BitNot, Folded::Int(value)) => Folded::Int(!value),
        _ => return None,
    })
}

/// Folds one scalar or array cast whose PHP result is unambiguous.
fn fold_cast(target: EvalCastType, value: Folded) -> Option<Folded> {
    Some(match (target, value) {
        (EvalCastType::Bool, value) => Folded::Bool(value.truthy()),
        (EvalCastType::Int, Folded::Null) => Folded::Int(0),
        (EvalCastType::Int, Folded::Bool(value)) => Folded::Int(i64::from(value)),
        (EvalCastType::Int, value @ Folded::Int(_)) => value,
        (EvalCastType::Int, Folded::String(value)) => Folded::Int(decimal_integer(&value)?),
        (EvalCastType::Int, Folded::Array { empty }) => Folded::Int(i64::from(!empty)),
        (EvalCastType::Float, Folded::Null) => Folded::Float(0.0),
        (EvalCastType::Float, Folded::Bool(value)) => Folded::Float(f64::from(u8::from(value))),
        (EvalCastType::Float, Folded::Int(value)) => Folded::Float(value as f64),
        (EvalCastType::Float, value @ Folded::Float(_)) => value,
        (EvalCastType::String, value) => Folded::String(value.string_bytes()?),
        (EvalCastType::Array, Folded::Null) => Folded::Array { empty: true },
        (EvalCastType::Array, value @ Folded::Array { .. }) => value,
        (EvalCastType::Array, _) => Folded::Array { empty: false },
        _ => return None,
    })
}

/// Reads a string that is exactly an optionally signed decimal integer, or is empty.
fn decimal_integer(value: &[u8]) -> Option<i64> {
    if value.is_empty() {
        return Some(0);
    }
    std::str::from_utf8(value).ok()?.parse().ok()
}

/// Folds one binary operator over operands whose PHP result is exact.
fn fold_binary(op: EvalBinOp, left: Folded, right: Folded) -> Option<Folded> {
    use Folded::{Bool, Float, Int};
    Some(match (op, left, right) {
        (EvalBinOp::Concat, left, right) => {
            let mut bytes = left.string_bytes()?;
            bytes.extend(right.string_bytes()?);
            Folded::String(bytes)
        }
        (EvalBinOp::LogicalXor, left, right) => Bool(left.truthy() != right.truthy()),
        (EvalBinOp::StrictEq | EvalBinOp::StrictNotEq, left, right) => {
            let same = strict_equal(&left, &right)?;
            Bool(same == (op == EvalBinOp::StrictEq))
        }
        (EvalBinOp::Add, Int(left), Int(right)) => left.checked_add(right).map_or(Float(left as f64 + right as f64), Int),
        (EvalBinOp::Sub, Int(left), Int(right)) => left.checked_sub(right).map_or(Float(left as f64 - right as f64), Int),
        (EvalBinOp::Mul, Int(left), Int(right)) => left.checked_mul(right).map_or(Float(left as f64 * right as f64), Int),
        (EvalBinOp::Div, Int(left), Int(right)) if right != 0 => match left.checked_rem(right) {
            Some(0) => Int(left.checked_div(right)?),
            _ => Float(left as f64 / right as f64),
        },
        (EvalBinOp::Mod, Int(left), Int(right)) if right != 0 => Int(left.checked_rem(right).unwrap_or(0)),
        (EvalBinOp::BitAnd, Int(left), Int(right)) => Int(left & right),
        (EvalBinOp::BitOr, Int(left), Int(right)) => Int(left | right),
        (EvalBinOp::BitXor, Int(left), Int(right)) => Int(left ^ right),
        (EvalBinOp::ShiftLeft, Int(left), Int(right)) if (0..64).contains(&right) => Int(left.wrapping_shl(right as u32)),
        (EvalBinOp::ShiftRight, Int(left), Int(right)) if (0..64).contains(&right) => Int(left >> right),
        (op @ (EvalBinOp::Add | EvalBinOp::Sub | EvalBinOp::Mul | EvalBinOp::Div), left, right) => {
            let (left, right) = (number(&left)?, number(&right)?);
            match op {
                EvalBinOp::Add => Float(left + right),
                EvalBinOp::Sub => Float(left - right),
                EvalBinOp::Mul => Float(left * right),
                _ if right != 0.0 => Float(left / right),
                _ => return None,
            }
        }
        (op, left, right) => {
            let ordering = numeric_ordering(&left, &right)?;
            match op {
                EvalBinOp::LooseEq => Bool(ordering.is_eq()),
                EvalBinOp::LooseNotEq => Bool(!ordering.is_eq()),
                EvalBinOp::Lt => Bool(ordering.is_lt()),
                EvalBinOp::LtEq => Bool(ordering.is_le()),
                EvalBinOp::Gt => Bool(ordering.is_gt()),
                EvalBinOp::GtEq => Bool(ordering.is_ge()),
                EvalBinOp::Spaceship => Int(ordering as i64),
                _ => return None,
            }
        }
    })
}

/// Returns an integer or float operand as a float, refusing every other type.
fn number(value: &Folded) -> Option<f64> {
    match value {
        Folded::Int(value) => Some(*value as f64),
        Folded::Float(value) => Some(*value),
        _ => None,
    }
}

/// Compares two numbers, or two booleans, or two nulls; other pairs are not folded.
fn numeric_ordering(left: &Folded, right: &Folded) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Folded::Int(left), Folded::Int(right)) => Some(left.cmp(right)),
        (Folded::Bool(left), Folded::Bool(right)) => Some(left.cmp(right)),
        (Folded::Null, Folded::Null) => Some(std::cmp::Ordering::Equal),
        _ => number(left)?.partial_cmp(&number(right)?),
    }
}

/// Returns PHP's `===` for two folded values; arrays are not compared.
fn strict_equal(left: &Folded, right: &Folded) -> Option<bool> {
    if matches!(left, Folded::Array { .. }) || matches!(right, Folded::Array { .. }) {
        return None;
    }
    Some(left == right)
}
