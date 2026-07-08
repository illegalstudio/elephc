//! Purpose:
//! Declarative eval registry entry for `hash_update`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - Hash context resources are owned by the eval context stream table.

eval_builtin! {
    name: "hash_update",
    area: String,
    params: [context, data],
    direct: HashContext,
    values: HashContext,
}

use super::super::super::*;

/// Evaluates PHP `hash_update($context, $data)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_update(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hash_context, data] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let hash_context = eval_expr(hash_context, context, scope, values)?;
    let data = eval_expr(data, context, scope, values)?;
    eval_hash_update_result(hash_context, data, context, values)
}

/// Feeds data into a materialized incremental hash context.
pub(in crate::interpreter) fn eval_hash_update_result(
    hash_context: RuntimeCellHandle,
    data: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = super::hash_init::eval_hash_context_resource_id(hash_context, values)?;
    let data = values.string_bytes(data)?;
    values.bool_value(context.stream_resources_mut().update_hash_context(id, &data))
}

/// Dispatches evaluated `hash_update()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_hash_update_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [hash_context, data] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_hash_update_result(*hash_context, *data, context, values)
}
