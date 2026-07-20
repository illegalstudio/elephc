//! Purpose:
//! Eval registry entry and implementation for `floor`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime numeric coercions stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "floor",
    area: Math,
    params: [num],
    direct: Floor,
    values: Floor,
}

/// Evaluates PHP `floor()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_floor_result(num, values)
}

/// Applies PHP `floor()` to one already evaluated value.
pub(in crate::interpreter) fn eval_floor_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.floor(num)
}
