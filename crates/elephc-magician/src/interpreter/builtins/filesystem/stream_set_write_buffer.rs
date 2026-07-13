//! Purpose:
//! Declarative eval registry entry for `stream_set_write_buffer`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream buffer-setting helper.

eval_builtin! {
    name: "stream_set_write_buffer",
    area: Filesystem,
    params: [stream, size],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_set_write_buffer` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_write_buffer_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_set_chunk_size::eval_builtin_stream_set_buffer_like("stream_set_write_buffer", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_set_write_buffer` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_write_buffer_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, size] => super::stream_set_chunk_size::eval_stream_set_buffer_like_result("stream_set_write_buffer", *stream, *size, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
