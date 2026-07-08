//! Purpose:
//! Eval registry entry and implementation for `log10`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "log10",
    area: Math,
    params: [num],
    direct: Log10,
    values: Log10,
}

/// Evaluates PHP `log10()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_log10(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_log10_result(num, values)
}

/// Applies PHP `log10()` to one already evaluated value.
pub(in crate::interpreter) fn eval_log10_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.log10())
}
