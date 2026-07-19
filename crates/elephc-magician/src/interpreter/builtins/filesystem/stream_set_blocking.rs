//! Purpose:
//! Declarative eval registry entry for `stream_set_blocking`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream blocking-mode helper.

eval_builtin! {
    name: "stream_set_blocking",
    area: Filesystem,
    params: [stream, enable],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_set_blocking` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_blocking_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_set_blocking(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_set_blocking` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_blocking_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, enable] => eval_stream_set_blocking_result(*stream, *enable, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
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
    let id = eval_stream_resource_id(stream, values)?;
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
