//! Purpose:
//! Eval registry entry and implementation for `atan`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "atan",
    area: Math,
    params: [num],
    direct: Atan,
    values: Atan,
}

/// Evaluates PHP `atan()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_atan(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_atan_result(num, values)
}

/// Applies PHP `atan()` to one already evaluated value.
pub(in crate::interpreter) fn eval_atan_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.atan())
}
