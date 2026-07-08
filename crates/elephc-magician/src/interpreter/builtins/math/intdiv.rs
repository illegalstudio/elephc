//! Purpose:
//! Eval registry entry and implementation for `intdiv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Division by zero and overflowing signed division remain runtime fatals.

use super::super::super::*;

eval_builtin! {
    name: "intdiv",
    area: Math,
    params: [num1, num2],
    direct: Intdiv,
    values: Intdiv,
}

/// Evaluates PHP `intdiv()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_intdiv(
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
    eval_intdiv_result(left, right, values)
}

/// Applies PHP `intdiv()` to two already evaluated values.
pub(in crate::interpreter) fn eval_intdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_int_value(left, values)?;
    let right = eval_int_value(right, values)?;
    if right == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let result = left.checked_div(right).ok_or(EvalStatus::RuntimeFatal)?;
    values.int(result)
}
