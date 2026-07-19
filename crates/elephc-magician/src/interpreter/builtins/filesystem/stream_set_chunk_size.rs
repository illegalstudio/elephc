//! Purpose:
//! Declarative eval registry entry for `stream_set_chunk_size`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream chunk-size metadata helper.

eval_builtin! {
    name: "stream_set_chunk_size",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_set_chunk_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_chunk_size_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_set_buffer_like("stream_set_chunk_size", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_set_chunk_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_chunk_size_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, size] => eval_stream_set_buffer_like_result("stream_set_chunk_size", *stream, *size, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates chunk/read/write buffer setting builtins.
pub(in crate::interpreter) fn eval_builtin_stream_set_buffer_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, size] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let size = eval_expr(size, context, scope, values)?;
    eval_stream_set_buffer_like_result(name, stream, size, context, values)
}

/// Applies a materialized chunk/read/write buffer setting.
pub(in crate::interpreter) fn eval_stream_set_buffer_like_result(
    name: &str,
    stream: RuntimeCellHandle,
    size: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let size = eval_int_value(size, values)?;
    match name {
        "stream_set_chunk_size" => match context.stream_resources_mut().set_chunk_size(id, size) {
            Some(previous) => values.int(previous),
            None => values.bool_value(false),
        },
        "stream_set_read_buffer" | "stream_set_write_buffer" => {
            match context.stream_resources().set_buffer(id, size) {
                Some(status) => values.int(status),
                None => values.bool_value(false),
            }
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
