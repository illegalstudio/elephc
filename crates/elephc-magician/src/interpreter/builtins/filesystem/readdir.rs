//! Purpose:
//! Declarative eval registry entry for `readdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the directory resource read helper.

eval_builtin! {
    contract: "readdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `readdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::closedir::eval_builtin_unary_directory("readdir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `readdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_readdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::closedir::eval_unary_directory_values_result("readdir", evaluated_args, context, values)
}
