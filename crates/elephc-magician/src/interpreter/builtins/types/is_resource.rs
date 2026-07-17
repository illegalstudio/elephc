//! Purpose:
//! Eval registry entry and implementation for `is_resource`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - The predicate reads the runtime tag directly and returns a PHP boolean.

use super::super::super::*;

eval_builtin! {
    name: "is_resource",
    area: Types,
    params: [value],
    direct: IsResource,
    values: IsResource,
}

/// Evaluates PHP `is_resource()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_is_resource(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_is_resource_result(value, values)
}

/// Applies PHP `is_resource()` to one already evaluated value.
pub(in crate::interpreter) fn eval_is_resource_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.bool_value(tag == EVAL_TAG_RESOURCE)
}
