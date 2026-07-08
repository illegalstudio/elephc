//! Purpose:
//! Declarative eval registry entry for `stream_context_set_option`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The signature keeps the existing two-argument and four-argument forms.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_set_option",
    area: Filesystem,
    params: [
        context,
        wrapper_or_options,
        option_name = EvalBuiltinDefaultValue::Null,
        value = EvalBuiltinDefaultValue::Null
    ],
    required: 2,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_context_set_option` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_context_set_option", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_context_set_option` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_context_set_option_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_context_set_option", evaluated_args, context, values)
}
