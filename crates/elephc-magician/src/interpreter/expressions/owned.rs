//! Purpose:
//! Produces independent owners for expression results crossing scope boundaries.
//! Selected branches keep PHP value semantics before activation owners are released.
//!
//! Called from:
//! - The expression evaluator when storing, returning, throwing, or discarding a value.
//!
//! Key details:
//! - A PHP value copy detaches a reference while preserving object and resource identity.
//! - Legacy reference getters can borrow storage and require an explicit retained read.

use super::*;
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};

/// Writes an independently owned expression and releases its source and string conversion on every path.
pub(in crate::interpreter) fn eval_output_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let original = eval_owned_expr(expr, context, scope, values)?;
    eval_output_value(original, context, values)
}

/// Consumes one output owner through the shared object string adapter and balanced cleanup.
pub(in crate::interpreter) fn eval_output_value(
    original: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
    let owners = StagedRuntimeOwners::default();
    owners.stage(original);
    with_runtime_owners(&owners, || {
        let operation = (|| {
            let value = eval_string_context_value(original, context, values)?;
            if value != original { owners.stage(value); }
            values.echo(value)
        })();
        let cleanup = owners.take_all().release(
            ReleaseOrder::Reverse,
            CleanupErrorPolicy::Last,
            |value| eval_release_value(context, values, value),
        );
        operation.and(cleanup)
    })
}

/// Keeps a copied receiver alive through a read or invocation, then releases it on every path.
pub(super) fn with_owned_receiver<V: RuntimeValueOps>(
    object: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut V,
    read: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = eval_owned_expr(object, context, scope, values)?;
    let object = RuntimeOwnerSlot::new(object);
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&object, &output), || {
        let operation = read(object.borrowed(), context, scope, values).and_then(|value| {
            let value = if value.is_borrowed() { values.retain(value)? } else { value };
            output.replace(Some(value));
            Ok(())
        });
        let cleanup = eval_release_value(context, values, object.take());
        output.finish(operation.and(cleanup), |value| eval_release_value(context, values, value))
    })
}

/// Copies a variable's value and preserves array side metadata.
pub(in crate::interpreter) fn copy_scope_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let copied = if values.is_reference(value)? {
        values.copy_value(value)?
    } else if values.is_array_like(value)? {
        values.copy_array_value(value)?
    } else {
        values.retain(value)?
    };
    if copied == value { return Ok(copied); }
    context.copy_array_metadata(value, copied);
    if let Err(status) = context.copy_pcntl_foreign_callable(value, copied, values) {
        context.clear_array_metadata(copied);
        let _ = values.release(copied);
        return Err(status);
    }
    Ok(copied)
}

/// Selects an ordinary or owned instance-property read without changing access checks.
pub(super) fn eval_property_get_with_result_ownership(
    object: RuntimeCellHandle,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    own_result: bool,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !own_result { return eval_property_get_result(object, property, context, values); }
    eval_owned_read(context, values, |context, values, owners| {
        eval_property_get_result_with_ownership(object, property, context, values, Some(owners))
    })
}

/// Selects an ordinary or owned static-property read without changing access checks.
pub(super) fn eval_static_property_get_with_result_ownership(
    class_name: &str,
    property: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    own_result: bool,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !own_result { return eval_static_property_get_result(class_name, property, context, values); }
    eval_owned_read(context, values, |context, values, owners| {
        eval_static_property_get_result_with_ownership(class_name, property, context, values, Some(owners))
    })
}

/// Detaches references and array boxes for value reads, preserving ordinary object identity.
fn eval_owned_read<V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    read: impl FnOnce(&mut ElephcEvalContext, &mut V, &StagedRuntimeOwners) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
    let owners = StagedRuntimeOwners::default();
    let output = RuntimeOwnerSlot::empty();
    with_runtime_owners(&(&owners, &output), || {
        let operation = read(context, values, &owners).and_then(|value| {
            // The getter owns a lease, but its boxed array can still be the property's box.
            // Copy the box so a later COW write replaces only the by-value result's payload.
            // Register the owner before probing, since a failed probe must release it too.
            let value = if value.is_borrowed() { values.retain(value)? } else { value };
            owners.stage(value);
            let value = if values.is_reference(value)? || values.is_array_like(value)? {
                copy_scope_value(value, context, values)?
            } else {
                owners.take_last().ok_or(EvalStatus::RuntimeFatal)?
            };
            output.replace(Some(value));
            Ok(())
        });
        let cleanup = owners.take_all().release(
            ReleaseOrder::Reverse,
            CleanupErrorPolicy::Last,
            |value| eval_release_value(context, values, value),
        );
        output.finish(operation.and(cleanup), |value| eval_release_value(context, values, value))
    })
}
