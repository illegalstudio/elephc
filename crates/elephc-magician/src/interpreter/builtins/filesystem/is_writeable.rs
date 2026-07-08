//! Purpose:
//! Declarative eval registry entry for `is_writeable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-probe helper.

eval_builtin! {
    name: "is_writeable",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `is_writeable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_writeable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("is_writeable", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `is_writeable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_writeable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("is_writeable", evaluated_args, context, values)
}
