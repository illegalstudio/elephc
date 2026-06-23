//! Purpose:
//! Implements eval incremental hash context builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - HashContext resources are owned by the eval resource table and backed by
//!   elephc-crypto opaque handles.
//! - HMAC streaming mode is intentionally not supported, matching the main type checker.

use super::super::super::*;
use super::super::*;

/// Evaluates PHP `hash_init($algo)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_hash_init(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [algo] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let algo = eval_expr(algo, context, scope, values)?;
    eval_hash_init_result(algo, context, values)
}

/// Opens an incremental hash context resource.
pub(in crate::interpreter) fn eval_hash_init_result(
    algo: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let algo = values.string_bytes(algo)?;
    match context.stream_resources_mut().open_hash_context(&algo) {
        Some(id) => values.resource(id),
        None => Err(EvalStatus::RuntimeFatal),
    }
}

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
    let id = eval_hash_context_resource_id(hash_context, values)?;
    let data = values.string_bytes(data)?;
    values.bool_value(context.stream_resources_mut().update_hash_context(id, &data))
}

/// Evaluates PHP `hash_final($context, $binary = false)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_hash_final(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(1..=2).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let hash_context = eval_expr(&args[0], context, scope, values)?;
    let binary = match args.get(1) {
        Some(binary) => {
            let binary = eval_expr(binary, context, scope, values)?;
            values.truthy(binary)?
        }
        None => false,
    };
    eval_hash_final_result(hash_context, binary, context, values)
}

/// Finalizes a materialized incremental hash context.
pub(in crate::interpreter) fn eval_hash_final_result(
    hash_context: RuntimeCellHandle,
    binary: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_hash_context_resource_id(hash_context, values)?;
    let raw = context
        .stream_resources_mut()
        .finalize_hash_context(id)
        .ok_or(EvalStatus::RuntimeFatal)?;
    eval_format_digest_result(&raw, binary, values)
}

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
    let id = eval_hash_context_resource_id(hash_context, values)?;
    match context.stream_resources_mut().copy_hash_context(id) {
        Some(copy_id) => values.resource(copy_id),
        None => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts a runtime resource cell into eval's zero-based hash context id.
fn eval_hash_context_resource_id(
    hash_context: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(hash_context)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(hash_context, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}
