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
    super::chdir::eval_builtin_unary_path_bool("rmdir", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `rmdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_rmdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [path] => super::chdir::eval_unary_path_bool_result("rmdir", *path, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
