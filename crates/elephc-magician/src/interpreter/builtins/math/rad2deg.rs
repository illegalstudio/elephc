//! Purpose:
//! Eval registry entry and implementation for `rad2deg`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "rad2deg",
    area: Math,
    params: [num],
    direct: Rad2deg,
    values: Rad2deg,
}

/// Evaluates PHP `rad2deg()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_rad2deg(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_rad2deg_result(num, values)
}

/// Applies PHP `rad2deg()` to one already evaluated value.
pub(in crate::interpreter) fn eval_rad2deg_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.to_degrees())
}
