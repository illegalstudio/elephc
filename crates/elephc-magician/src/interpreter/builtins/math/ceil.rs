//! Purpose:
//! Eval registry entry and implementation for `ceil`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime numeric coercions stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "ceil",
    area: Math,
    params: [num],
    direct: Ceil,
    values: Ceil,
}

/// Evaluates PHP `ceil()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_ceil_result(num, values)
}

/// Applies PHP `ceil()` to one already evaluated value.
pub(in crate::interpreter) fn eval_ceil_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.ceil(num)
}
