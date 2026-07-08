//! Purpose:
//! Declarative eval registry entry for `filetype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the filetype helper.

eval_builtin! {
    name: "filetype",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `filetype` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filetype_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("filetype", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `filetype` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_filetype_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("filetype", evaluated_args, context, values)
}
