//! Purpose:
//! Declarative eval registry entry for `stream_isatty`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream descriptor predicate helper.

eval_builtin! {
    name: "stream_isatty",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_isatty` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_isatty_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_isatty(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_isatty` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_isatty_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream] => eval_stream_isatty_result(*stream, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

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
    let id = eval_stream_resource_id(stream, values)?;
    values.bool_value(context.stream_resources().isatty(id).unwrap_or(false))
}
