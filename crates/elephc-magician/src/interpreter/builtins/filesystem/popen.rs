//! Purpose:
//! Declarative eval registry entry for `popen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the process-pipe open helper.

eval_builtin! {
    name: "popen",
    area: Filesystem,
    params: [command, mode],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `popen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_popen_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("popen", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `popen` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_popen_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("popen", evaluated_args, context, values)
}
