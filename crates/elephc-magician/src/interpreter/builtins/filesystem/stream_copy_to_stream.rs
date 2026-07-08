//! Purpose:
//! Declarative eval registry entry for `stream_copy_to_stream`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream copy helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_copy_to_stream",
    area: Filesystem,
    params: [
        from,
        to,
        length = EvalBuiltinDefaultValue::Null,
        offset = EvalBuiltinDefaultValue::Int(-1)
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_copy_to_stream` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_copy_to_stream", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_copy_to_stream` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_copy_to_stream_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_copy_to_stream", evaluated_args, context, values)
}
