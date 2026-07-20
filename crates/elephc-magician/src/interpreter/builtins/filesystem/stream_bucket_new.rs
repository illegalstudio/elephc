//! Purpose:
//! Declarative eval registry entry and implementation for `stream_bucket_new`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Creates stdClass bucket objects with `data` and `datalen` properties.

eval_builtin! {
    name: "stream_bucket_new",
    area: Filesystem,
    params: [stream, buffer],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_bucket_new($stream, $buffer)`.
pub(in crate::interpreter) fn eval_stream_bucket_new_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, buffer] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let buffer = eval_expr(buffer, context, scope, values)?;
    eval_stream_bucket_new_result(stream, buffer, context, values)
}

/// Creates a bucket object from already evaluated stream and buffer arguments.
pub(in crate::interpreter) fn eval_stream_bucket_new_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, buffer] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_bucket_new_result(*stream, *buffer, context, values)
}

/// Creates a stdClass-backed stream bucket object.
pub(in crate::interpreter) fn eval_stream_bucket_new_result(
    stream: RuntimeCellHandle,
    buffer: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let stream_id = eval_stream_extension_resource_id(stream, values)?;
    if !context.stream_resources().has_stream(stream_id) {
        return values.null();
    }
    let bytes = values.string_bytes(buffer)?;
    let bucket = values.new_object("stdClass")?;
    let data = values.string_bytes_value(&bytes)?;
    let datalen = values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    values.property_set(bucket, "data", data)?;
    values.property_set(bucket, "datalen", datalen)?;
    Ok(bucket)
}

/// Converts a runtime resource cell into eval's zero-based stream-extension id.
pub(in crate::interpreter) fn eval_stream_extension_resource_id(
    resource: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(resource)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(resource, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
