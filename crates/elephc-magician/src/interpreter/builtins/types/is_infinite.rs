//! Purpose:
//! Eval registry entry and implementation for `is_infinite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced with eval numeric semantics before the float check.

use super::super::super::*;

eval_builtin! {
    name: "is_infinite",
    area: Types,
    params: [num],
    direct: IsInfinite,
    values: IsInfinite,
}

/// Evaluates PHP `is_infinite()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_infinite(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_is_infinite_result(num, values)
}

/// Applies PHP `is_infinite()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_infinite_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = eval_float_value(num, values)?.is_infinite();
    values.bool_value(result)
}
