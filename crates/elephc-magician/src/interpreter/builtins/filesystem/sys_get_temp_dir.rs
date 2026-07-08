//! Purpose:
//! Declarative eval registry entry for `sys_get_temp_dir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the temporary-directory helper.

eval_builtin! {
    name: "sys_get_temp_dir",
    area: Filesystem,
    params: [],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `sys_get_temp_dir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("sys_get_temp_dir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `sys_get_temp_dir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_sys_get_temp_dir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("sys_get_temp_dir", evaluated_args, context, values)
}
