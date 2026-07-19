//! Purpose:
//! Declarative eval registry entry for `disk_total_space`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the disk-space helper.

eval_builtin! {
    name: "disk_total_space",
    area: Filesystem,
    params: [directory],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `disk_total_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_total_space_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::disk_free_space::eval_builtin_disk_space("disk_total_space", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `disk_total_space` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_disk_total_space_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [directory] => super::disk_free_space::eval_disk_space_result("disk_total_space", *directory, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
