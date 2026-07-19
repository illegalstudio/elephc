//! Purpose:
//! Eval registry entry and implementation for `hypot`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Both arguments are evaluated in source order before float coercion.

use super::super::super::*;

eval_builtin! {
    name: "hypot",
    area: Math,
    params: [x, y],
    direct: Hypot,
    values: Hypot,
}

/// Evaluates PHP `hypot()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_hypot(
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
    eval_hypot_result(left, right, values)
}

/// Applies PHP `hypot()` to two already evaluated values.
pub(in crate::interpreter) fn eval_hypot_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_float_value(left, values)?;
    let right = eval_float_value(right, values)?;
    values.float(left.hypot(right))
}
