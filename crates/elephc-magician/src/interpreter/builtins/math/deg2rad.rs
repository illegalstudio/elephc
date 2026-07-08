//! Purpose:
//! Eval registry entry and implementation for `deg2rad`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "deg2rad",
    area: Math,
    params: [num],
    direct: Deg2rad,
    values: Deg2rad,
}

/// Evaluates PHP `deg2rad()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_deg2rad(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_deg2rad_result(num, values)
}

/// Applies PHP `deg2rad()` to one already evaluated value.
pub(in crate::interpreter) fn eval_deg2rad_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.to_radians())
}
