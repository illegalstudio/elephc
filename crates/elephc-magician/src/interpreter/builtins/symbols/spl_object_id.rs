//! Purpose:
//! Declarative eval registry entry for `spl_object_id`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL object identity helper.

eval_builtin! {
    name: "spl_object_id",
    area: Symbols,
    params: [object],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_object_id` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_object_id_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_spl_object_identity("spl_object_id", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_object_id` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_object_id_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args { [object] => super::super::eval_spl_object_identity_result("spl_object_id", *object, values), _ => Err(EvalStatus::RuntimeFatal), }
}
