//! Purpose:
//! Declarative eval registry entry for `stream_context_get_default`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the default stream context helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_get_default",
    area: Filesystem,
    params: [options = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_context_get_default` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_get_default_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_context_get_default", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_context_get_default` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_get_default_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_context_get_default", evaluated_args, context, values)
}
