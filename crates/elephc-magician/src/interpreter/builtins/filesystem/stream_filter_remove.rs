//! Purpose:
//! Declarative eval registry entry and implementation for `stream_filter_remove`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Closes eval-local filter resources created by append/prepend.

eval_builtin! {
    name: "stream_filter_remove",
    area: Filesystem,
    params: [stream_filter],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_filter_remove($stream_filter)`.
pub(in crate::interpreter) fn eval_stream_filter_remove_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_filter] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream_filter = eval_expr(stream_filter, context, scope, values)?;
    eval_stream_filter_remove_result(stream_filter, context, values)
}

/// Removes an already evaluated eval-local filter resource.
pub(in crate::interpreter) fn eval_stream_filter_remove_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream_filter] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_filter_remove_result(*stream_filter, context, values)
}

/// Removes an eval-local filter resource.
pub(in crate::interpreter) fn eval_stream_filter_remove_result(
    stream_filter: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::stream_bucket_new::eval_stream_extension_resource_id(stream_filter, values)?;
    values.bool_value(context.stream_resources_mut().close_filter_resource(id))
}
