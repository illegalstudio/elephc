//! Purpose:
//! Eval registry entry and implementation for `boolval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Cast behavior is implemented here; shared scalar coercions still flow
//!   through `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "boolval",
    area: Types,
    params: [value],
    direct: Boolval,
    values: Boolval,
}

/// Evaluates PHP `boolval()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_boolval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_boolval_result(value, values)
}

/// Applies PHP `boolval()` to one already evaluated value.
pub(in crate::interpreter) fn eval_boolval_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.cast_bool(value)
}
