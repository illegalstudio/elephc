//! Purpose:
//! Eval registry entry and implementation for `is_nan`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced with eval numeric semantics before the float check.

use super::super::super::*;

eval_builtin! {
    name: "is_nan",
    area: Types,
    params: [num],
    direct: IsNan,
    values: IsNan,
}

/// Evaluates PHP `is_nan()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_nan(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_is_nan_result(num, values)
}

/// Applies PHP `is_nan()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_nan_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = eval_float_value(num, values)?.is_nan();
    values.bool_value(result)
}
