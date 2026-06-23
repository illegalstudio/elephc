//! Purpose:
//! Implements small numeric math wrapper builtins for eval execution.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting` re-exports.
//!
//! Key details:
//! - Argument evaluation stays in PHP source order before delegating scalar math
//!   behavior to `RuntimeValueOps`.

use super::super::super::*;

/// Evaluates PHP's `ceil(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.ceil(value)
}

/// Evaluates PHP's `floor(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.floor(value)
}

/// Evaluates PHP's zero-argument `pi()` builtin.
pub(in crate::interpreter) fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.float(std::f64::consts::PI)
}

/// Evaluates PHP's `pow(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_pow(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    values.pow(left, right)
}

/// Evaluates PHP's `round(...)` over one value and an optional precision expression.
pub(in crate::interpreter) fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            values.round(value, None)
        }
        [value, precision] => {
            let value = eval_expr(value, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            values.round(value, Some(precision))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
