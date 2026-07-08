//! Purpose:
//! Eval registry entry and implementation for `cosh`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced through eval numeric semantics before applying
//!   the Rust `f64` operation matching PHP's math behavior.

use super::super::super::*;

eval_builtin! {
    name: "cosh",
    area: Math,
    params: [num],
    direct: Cosh,
    values: Cosh,
}

/// Evaluates PHP `cosh()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_cosh(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_cosh_result(num, values)
}

/// Applies PHP `cosh()` to one already evaluated value.
pub(in crate::interpreter) fn eval_cosh_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let num = eval_float_value(num, values)?;
    values.float(num.cosh())
}
