//! Purpose:
//! Eval registry entry and implementation for `gettype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime tags are mapped to PHP's historical `gettype()` names.

use super::super::super::*;

eval_builtin! {
    contract: "gettype",
    area: Types,
    direct: Gettype,
    values: Gettype,
}

/// Evaluates PHP `gettype()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result_with_context(value, context, values)
}

/// Converts one eval value into PHP's `gettype()` spelling, including the closed-resource name.
pub(in crate::interpreter) fn eval_gettype_result_with_context(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_RESOURCE
        && super::super::scalars::eval_resource_is_closed(value, context, values)?
    {
        return values.string("resource (closed)");
    }
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
pub(in crate::interpreter) fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}
