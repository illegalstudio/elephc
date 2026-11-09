//! Purpose:
//! Eval registry entry and implementation for `get_debug_type`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Scalar tags map to PHP 8's debug spellings, objects resolve their eval-visible class
//!   name, and resources distinguish open stream/context/filter handles from closed ones.

use super::super::super::*;

eval_builtin! {
    contract: "get_debug_type",
    area: Types,
    direct: GetDebugType,
    values: GetDebugType,
}

/// Evaluates PHP `get_debug_type()` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_get_debug_type(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_get_debug_type_result(value, context, values)
}

/// Converts one eval value into PHP's `get_debug_type()` spelling.
pub(in crate::interpreter) fn eval_get_debug_type_result(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    match tag {
        EVAL_TAG_INT => values.string("int"),
        EVAL_TAG_STRING => values.string("string"),
        EVAL_TAG_FLOAT => values.string("float"),
        EVAL_TAG_BOOL => values.string("bool"),
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => values.string("array"),
        EVAL_TAG_OBJECT => super::super::symbols::eval_get_class_result(value, context, values),
        EVAL_TAG_CALLABLE => values.string("Closure"),
        EVAL_TAG_RESOURCE => {
            if super::super::scalars::eval_resource_is_closed(value, context, values)? {
                return values.string("resource (closed)");
            }
            let kind = super::super::scalars::eval_resource_type_name(value, context, values)?;
            let label = match kind {
                "stream-context" => "resource (stream-context)",
                "stream filter" => "resource (stream filter)",
                _ => "resource (stream)",
            };
            values.string(label)
        }
        _ => values.string("null"),
    }
}
