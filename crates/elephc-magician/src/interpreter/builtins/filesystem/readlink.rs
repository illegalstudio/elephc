//! Purpose:
//! Declarative eval registry entry for `readlink`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the symbolic-link target helper.

eval_builtin! {
    name: "readlink",
    area: Filesystem,
    params: [path],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `readlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readlink_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("readlink", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `readlink` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readlink_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("readlink", evaluated_args, context, values)
}
