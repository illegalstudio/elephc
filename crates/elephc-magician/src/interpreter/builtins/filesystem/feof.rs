//! Purpose:
//! Declarative eval registry entry for `feof`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary stream helper.

eval_builtin! {
    name: "feof",
    area: Filesystem,
    params: [stream],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `feof` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_feof_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("feof", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `feof` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_feof_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("feof", evaluated_args, context, values)
}
