//! Purpose:
//! Eval registry entry and implementation for `intval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Cast behavior is implemented here; shared scalar coercions still flow
//!   through `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "intval",
    area: Types,
    params: [value],
    direct: Intval,
    values: Intval,
}

/// Evaluates PHP `intval()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_intval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_intval_result(value, values)
}

/// Applies PHP `intval()` to one already evaluated value.
pub(in crate::interpreter) fn eval_intval_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.cast_int(value)
}
