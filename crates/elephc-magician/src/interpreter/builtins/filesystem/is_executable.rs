//! Purpose:
//! Declarative eval registry entry for `is_executable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-probe helper.

eval_builtin! {
    name: "is_executable",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `is_executable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_executable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::file_exists::eval_builtin_file_probe("is_executable", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `is_executable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_executable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => super::file_exists::eval_file_probe_result("is_executable", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
