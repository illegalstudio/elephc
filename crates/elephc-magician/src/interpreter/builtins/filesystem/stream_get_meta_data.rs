//! Purpose:
//! Declarative eval registry entry for `stream_get_meta_data`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "stream_get_meta_data",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_get_meta_data` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_meta_data_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_get_meta_data(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_get_meta_data` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_get_meta_data_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_stream_get_meta_data_handle_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds PHP's stream metadata array for one eval-local stream resource.
pub(in crate::interpreter) fn eval_stream_get_meta_data_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(meta) = context.stream_resources().meta_data(id) else {
        return values.bool_value(false);
    };
    let mut result = values.assoc_new(9)?;
    result = eval_stream_meta_set_bool(result, "timed_out", false, values)?;
    result = eval_stream_meta_set_bool(result, "blocked", true, values)?;
    result = eval_stream_meta_set_bool(result, "eof", meta.eof, values)?;
    result = eval_stream_meta_set_string(result, "wrapper_type", "plainfile", values)?;
    result = eval_stream_meta_set_string(result, "stream_type", "STDIO", values)?;
    result = eval_stream_meta_set_string(result, "mode", &meta.mode, values)?;
    result = eval_stream_meta_set_int(result, "unread_bytes", 0, values)?;
    result = eval_stream_meta_set_bool(result, "seekable", true, values)?;
    eval_stream_meta_set_string(result, "uri", &meta.uri, values)
}

/// Inserts a boolean field into the stream metadata array.
fn eval_stream_meta_set_bool(
    array: RuntimeCellHandle,
    key: &str,
    value: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.bool_value(value)?;
    values.array_set(array, key, value)
}

/// Inserts an integer field into the stream metadata array.
fn eval_stream_meta_set_int(
    array: RuntimeCellHandle,
    key: &str,
    value: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.int(value)?;
    values.array_set(array, key, value)
}

/// Inserts a string field into the stream metadata array.
fn eval_stream_meta_set_string(
    array: RuntimeCellHandle,
    key: &str,
    value: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.string(key)?;
    let value = values.string(value)?;
    values.array_set(array, key, value)
}

/// Evaluates PHP `stream_get_meta_data($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_get_meta_data(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_stream_get_meta_data_handle_result(stream, context, values)
}

/// Builds PHP metadata for one materialized stream resource handle.
pub(in crate::interpreter) fn eval_stream_get_meta_data_handle_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    eval_stream_get_meta_data_result(id, context, values)
}
