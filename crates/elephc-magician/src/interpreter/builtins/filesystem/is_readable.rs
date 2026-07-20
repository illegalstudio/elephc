//! Purpose:
//! Declarative eval registry entry for `is_readable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the file-probe helper.

eval_builtin! {
    name: "is_readable",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `is_readable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_readable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::file_exists::eval_builtin_file_probe("is_readable", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `is_readable` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_readable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => super::file_exists::eval_file_probe_result("is_readable", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
