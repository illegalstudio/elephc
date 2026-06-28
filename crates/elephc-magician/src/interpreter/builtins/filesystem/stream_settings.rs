//! Purpose:
//! Implements eval stream descriptor predicate and setting builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - `stream_isatty` and `stream_set_blocking` call host libc for local files.
//! - Buffer and chunk-size settings are eval-local metadata; timeout is false for
//!   local files because the main backend applies it as a socket option.

use super::super::super::*;

/// Evaluates `stream_isatty($stream)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_isatty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    eval_stream_isatty_result(stream, context, values)
}

/// Returns whether a materialized stream resource is attached to a terminal.
pub(in crate::interpreter) fn eval_stream_isatty_result(
    stream: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_settings_stream_resource_id(stream, values)?;
    values.bool_value(context.stream_resources().isatty(id).unwrap_or(false))
}

/// Evaluates `stream_set_blocking($stream, $enable)`.
pub(in crate::interpreter) fn eval_builtin_stream_set_blocking(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, enable] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let enable = eval_expr(enable, context, scope, values)?;
    eval_stream_set_blocking_result(stream, enable, context, values)
}

/// Toggles blocking mode on a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_set_blocking_result(
    stream: RuntimeCellHandle,
    enable: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_settings_stream_resource_id(stream, values)?;
    let enable = values.truthy(enable)?;
    if let Some(result) = eval_user_wrapper_stream_set_option_result(
        id,
        EVAL_STREAM_OPTION_BLOCKING,
        if enable { 1 } else { 0 },
        0,
        context,
        values,
    )? {
        return Ok(result);
    }
    values.bool_value(context.stream_resources().set_blocking(id, enable).unwrap_or(false))
}

/// Evaluates `stream_set_timeout($stream, $seconds, $microseconds = 0)`.
pub(in crate::interpreter) fn eval_builtin_stream_set_timeout(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let seconds = eval_expr(&args[1], context, scope, values)?;
    let microseconds = match args.get(2) {
        Some(microseconds) => Some(eval_expr(microseconds, context, scope, values)?),
        None => None,
    };
    eval_stream_set_timeout_result(stream, seconds, microseconds, context, values)
}

/// Applies a timeout request to a materialized stream resource.
pub(in crate::interpreter) fn eval_stream_set_timeout_result(
    stream: RuntimeCellHandle,
    seconds: RuntimeCellHandle,
    microseconds: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_settings_stream_resource_id(stream, values)?;
    let seconds = eval_int_value(seconds, values)?;
    let microseconds = match microseconds {
        Some(microseconds) => eval_int_value(microseconds, values)?,
        None => 0,
    };
    if let Some(result) = eval_user_wrapper_stream_set_option_result(
        id,
        EVAL_STREAM_OPTION_READ_TIMEOUT,
        seconds,
        microseconds,
        context,
        values,
    )? {
        return Ok(result);
    }
    values.bool_value(
        context
            .stream_resources()
            .set_timeout(id, seconds, microseconds)
            .unwrap_or(false),
    )
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
    let id = eval_settings_stream_resource_id(stream, values)?;
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

/// Converts a runtime resource cell into eval's zero-based stream id.
fn eval_settings_stream_resource_id(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
