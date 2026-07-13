//! Purpose:
//! Declarative eval registry entry for `stream_set_timeout`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream timeout-setting helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_set_timeout",
    area: Filesystem,
    params: [stream, seconds, microseconds = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_set_timeout` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_timeout_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_set_timeout(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_set_timeout` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_timeout_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, seconds] => eval_stream_set_timeout_result(*stream, *seconds, None, context, values),
        [stream, seconds, microseconds] => eval_stream_set_timeout_result(*stream, *seconds, Some(*microseconds), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
    let id = eval_stream_resource_id(stream, values)?;
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
