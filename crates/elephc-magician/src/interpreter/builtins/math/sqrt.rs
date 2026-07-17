//! Purpose:
//! Eval registry entry and implementation for `sqrt`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime numeric coercions stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "sqrt",
    area: Math,
    params: [num],
    direct: Sqrt,
    values: Sqrt,
}

/// Evaluates PHP `sqrt()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_sqrt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_sqrt_result(num, values)
}

/// Applies PHP `sqrt()` to one already evaluated value.
pub(in crate::interpreter) fn eval_sqrt_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.sqrt(num)
}
