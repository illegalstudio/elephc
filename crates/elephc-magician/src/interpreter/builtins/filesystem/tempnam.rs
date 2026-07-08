//! Purpose:
//! Declarative eval registry entry for `tempnam`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the temporary-name helper.

eval_builtin! {
    name: "tempnam",
    area: Filesystem,
    params: [directory, prefix],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `tempnam` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tempnam_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("tempnam", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `tempnam` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_tempnam_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("tempnam", evaluated_args, context, values)
}
