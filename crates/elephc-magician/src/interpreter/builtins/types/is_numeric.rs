//! Purpose:
//! Eval registry entry and implementation for `is_numeric`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Numeric strings follow the legacy ASCII scan shared with the static backend.

use super::super::super::*;

eval_builtin! {
    name: "is_numeric",
    area: Types,
    params: [value],
    direct: IsNumeric,
    values: IsNumeric,
}

/// Evaluates PHP `is_numeric()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_numeric(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_is_numeric_result(value, values)
}

/// Applies PHP `is_numeric()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_numeric_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = tag == EVAL_TAG_INT
        || tag == EVAL_TAG_FLOAT
        || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?));
    values.bool_value(result)
}
