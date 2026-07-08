//! Purpose:
//! Eval registry entry and implementation for `tanh`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "tanh",
    area: Math,
    params: [num],
    direct: Tanh,
    values: Tanh,
}

/// Evaluates PHP `tanh()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_tanh(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_tanh_result(num, values)
}

/// Applies PHP `tanh()` to one already evaluated value.
pub(in crate::interpreter) fn eval_tanh_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.tanh())
}
