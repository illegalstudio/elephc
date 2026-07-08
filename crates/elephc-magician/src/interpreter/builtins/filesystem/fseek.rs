//! Purpose:
//! Declarative eval registry entry for `fseek`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream seek helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fseek",
    area: Filesystem,
    params: [stream, offset, whence = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fseek` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fseek_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("fseek", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fseek` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fseek_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("fseek", evaluated_args, context, values)
}
