//! Purpose:
//! Declarative eval registry entry for `hash_copy`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - Hash context resources are owned by the eval context stream table.

eval_builtin! {
    name: "hash_copy",
    area: String,
    params: [context],
    direct: HashContext,
    values: HashContext,
}

use super::super::super::*;

/// Evaluates PHP `hash_copy($context)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_hash_copy(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hash_context] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hash_context = eval_expr(hash_context, context, scope, values)?;
    eval_hash_copy_result(hash_context, context, values)
}

/// Clones a materialized incremental hash context into a new resource.
pub(in crate::interpreter) fn eval_hash_copy_result(
    hash_context: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::hash_init::eval_hash_context_resource_id(hash_context, values)?;
    match context.stream_resources_mut().copy_hash_context(id) {
        Some(copy_id) => values.resource(copy_id),
        None => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated `hash_copy()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_hash_copy_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hash_context] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_hash_copy_result(*hash_context, context, values)
}
