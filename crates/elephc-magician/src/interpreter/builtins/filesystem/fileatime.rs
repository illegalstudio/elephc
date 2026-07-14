//! Purpose:
//! Declarative eval registry entry for `fileatime`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the scalar stat helper.

eval_builtin! {
    name: "fileatime",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `fileatime` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fileatime_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stat::eval_builtin_file_stat_scalar("fileatime", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `fileatime` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_fileatime_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => super::stat::eval_file_stat_scalar_result("fileatime", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
