//! Purpose:
//! Declarative eval registry entry for `fprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Variadic values are formatted by the existing printf-family helper.

eval_builtin! {
    name: "fprintf",
    area: Filesystem,
    params: [stream, format],
    variadic: values,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fprintf_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("fprintf", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fprintf` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fprintf_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("fprintf", evaluated_args, context, values)
}
