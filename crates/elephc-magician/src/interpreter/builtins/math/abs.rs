//! Purpose:
//! Eval registry entry and implementation for `abs`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime scalar absolute-value coercions stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "abs",
    area: Math,
    params: [num],
    direct: Abs,
    values: Abs,
}

/// Evaluates PHP `abs()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_abs_result(num, values)
}

/// Applies PHP `abs()` to one already evaluated value.
pub(in crate::interpreter) fn eval_abs_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.abs(num)
}
