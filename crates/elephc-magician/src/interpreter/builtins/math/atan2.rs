//! Purpose:
//! Eval registry entry and implementation for `atan2`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Both arguments are evaluated in source order before float coercion.

use super::super::super::*;

eval_builtin! {
    name: "atan2",
    area: Math,
    params: [y, x],
    direct: Atan2,
    values: Atan2,
}

/// Evaluates PHP `atan2()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_atan2(
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
    eval_atan2_result(left, right, values)
}

/// Applies PHP `atan2()` to two already evaluated values.
pub(in crate::interpreter) fn eval_atan2_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_float_value(left, values)?;
    let right = eval_float_value(right, values)?;
    values.float(left.atan2(right))
}
