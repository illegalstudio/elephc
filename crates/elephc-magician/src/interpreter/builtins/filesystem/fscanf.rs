//! Purpose:
//! Declarative eval registry entry for `fscanf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The current eval implementation returns parsed values and ignores output vars.

eval_builtin! {
    name: "fscanf",
    area: Filesystem,
    params: [stream, format],
    variadic: vars,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("fscanf", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fscanf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fscanf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("fscanf", evaluated_args, context, values)
}
