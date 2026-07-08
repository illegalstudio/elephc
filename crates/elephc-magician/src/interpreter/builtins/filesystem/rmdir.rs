//! Purpose:
//! Declarative eval registry entry for `rmdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary path operation helper.

eval_builtin! {
    name: "rmdir",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `rmdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rmdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("rmdir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `rmdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rmdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("rmdir", evaluated_args, context, values)
}
