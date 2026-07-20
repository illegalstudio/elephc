//! Purpose:
//! Eval registry entry and implementation for `log2`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "log2",
    area: Math,
    params: [num],
    direct: Log2,
    values: Log2,
}

/// Evaluates PHP `log2()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_log2(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_log2_result(num, values)
}

/// Applies PHP `log2()` to one already evaluated value.
pub(in crate::interpreter) fn eval_log2_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.log2())
}
