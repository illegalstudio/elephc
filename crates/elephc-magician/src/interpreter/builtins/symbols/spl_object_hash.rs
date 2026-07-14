//! Purpose:
//! Eval registry entry for `spl_object_hash`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Object identity semantics are shared with `spl_object_id()`.

eval_builtin! {
    name: "spl_object_hash",
    area: Symbols,
    params: [object],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_object_hash(...)` calls through the `spl_object_id` owner.
pub(in crate::interpreter) fn eval_spl_object_hash_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_object_id::eval_builtin_spl_object_identity(
        "spl_object_hash",
        args,
        context,
        scope,
        values,
    )
}

/// Evaluates materialized `spl_object_hash(...)` arguments through the `spl_object_id` owner.
pub(in crate::interpreter) fn eval_spl_object_hash_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [object] => {
            super::spl_object_id::eval_spl_object_identity_result("spl_object_hash", *object, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
