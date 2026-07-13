//! Purpose:
//! Eval registry entry and implementation for `floatval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Cast behavior is implemented here; shared scalar coercions still flow
//!   through `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "floatval",
    area: Types,
    params: [value],
    direct: Floatval,
    values: Floatval,
}

/// Evaluates PHP `floatval()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_floatval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_floatval_result(value, values)
}

/// Applies PHP `floatval()` to one already evaluated value.
pub(in crate::interpreter) fn eval_floatval_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.cast_float(value)
}
