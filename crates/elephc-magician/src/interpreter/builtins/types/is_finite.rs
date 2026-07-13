//! Purpose:
//! Eval registry entry and implementation for `is_finite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The argument is coerced with eval numeric semantics before the float check.

use super::super::super::*;

eval_builtin! {
    name: "is_finite",
    area: Types,
    params: [num],
    direct: IsFinite,
    values: IsFinite,
}

/// Evaluates PHP `is_finite()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_finite(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [num] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let num = eval_expr(num, context, scope, values)?;
    eval_is_finite_result(num, values)
}

/// Applies PHP `is_finite()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_finite_result(
    num: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = eval_float_value(num, values)?.is_finite();
    values.bool_value(result)
}
