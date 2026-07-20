//! Purpose:
//! Declarative eval registry entry for `lstat`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stat-array helper.

eval_builtin! {
    name: "lstat",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `lstat` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_lstat_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stat::eval_builtin_stat_array("lstat", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `lstat` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_lstat_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => super::stat::eval_stat_array_result("lstat", *filename, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
