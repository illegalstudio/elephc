//! Purpose:
//! Declarative eval registry entry for `stream_get_meta_data`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    contract: "stream_get_meta_data",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
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
    let mut result = EvalArrayBuilder::assoc(values, 9)?;
    result.string("timed_out", |values| values.bool_value(false))?;
    result.string("blocked", |values| values.bool_value(true))?;
    result.string("eof", |values| values.bool_value(meta.eof))?;
    result.string("wrapper_type", |values| values.string("plainfile"))?;
    result.string("stream_type", |values| values.string("STDIO"))?;
    result.string("mode", |values| values.string(&meta.mode))?;
    result.string("unread_bytes", |values| values.int(0))?;
    result.string("seekable", |values| values.bool_value(true))?;
    result.string("uri", |values| values.string(&meta.uri))?;
    Ok(result.finish())
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
