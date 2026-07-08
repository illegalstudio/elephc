//! Purpose:
//! Declarative eval registry entry for `stream_context_set_params`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the accepted no-op helper.

eval_builtin! {
    name: "stream_context_set_params",
    area: Filesystem,
    params: [context, params],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_context_set_params` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_set_params_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_context_set_params", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_context_set_params` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_set_params_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_context_set_params", evaluated_args, context, values)
}
