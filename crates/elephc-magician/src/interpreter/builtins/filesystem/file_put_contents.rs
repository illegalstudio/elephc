//! Purpose:
//! Declarative eval registry entry for `file_put_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the one-shot file write helper.

eval_builtin! {
    name: "file_put_contents",
    area: Filesystem,
    params: [filename, data],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("file_put_contents", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("file_put_contents", evaluated_args, context, values)
}
