//! Purpose:
//! Declarative eval registry entry for `stream_filter_remove`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream filter removal helper.

eval_builtin! {
    name: "stream_filter_remove",
    area: Filesystem,
    params: [stream_filter],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_filter_remove` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_filter_remove_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_filter_remove", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_filter_remove` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_filter_remove_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_filter_remove", evaluated_args, context, values)
}
