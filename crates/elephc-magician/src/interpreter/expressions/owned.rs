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

/// Owns a read's temporary receiver and key while preserving a variable receiver's live identity.
pub(super) fn eval_owned_array_index_read(
    array: &EvalExpr, index: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_owned_read(context, values, |context, values, owners| {
        let array = if matches!(array, EvalExpr::LoadVar(_)) {
            eval_expr(array, context, scope, values)?
        } else {
            let value = eval_owned_expr(array, context, scope, values)?;
            owners.push(value);
            value
        };
        let index = eval_owned_expr(index, context, scope, values)?;
        owners.push(index);
        eval_owned_array_get_result(array, index, context, values)
    })
}

/// Owns concat operands and Stringable conversions, releasing source temporaries in PHP operand order.
pub(super) fn eval_owned_concat(
    left: &EvalExpr, right: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_owned_read(context, values, |context, values, owners| {
        let left = eval_owned_expr(left, context, scope, values)?;
        owners.push(left);
        let right = eval_owned_expr(right, context, scope, values)?;
        owners.push(right);
        // The common cleanup pops owners, but concat releases the left source operand first.
        owners.swap(0, 1);
        let left_string = eval_string_context_value(left, context, values)?;
        if left_string != left { owners.push(left_string); }
        let right_string = eval_string_context_value(right, context, values)?;
        if right_string != right { owners.push(right_string); }
        values.concat(left_string, right_string)
    })
}

/// Writes an independently owned expression and releases its source and string conversion on every path.
pub(in crate::interpreter) fn eval_output_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let original = eval_owned_expr(expr, context, scope, values)?;
    let mut converted = None;
    let mut result = (|| {
        let value = eval_string_context_value(original, context, values)?;
        if value != original { converted = Some(value); }
        values.echo(value)
    })();
    if let Some(value) = converted {
        if let Err(status) = eval_release_value(context, values, value) { result = Err(status); }
    }
    if let Err(status) = eval_release_value(context, values, original) { result = Err(status); }
    result
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
    let result = read(object, context, scope, values);
    let cleanup = eval_release_value(context, values, object);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
    }
}

/// Copies a variable's value and preserves array side metadata.
pub(super) fn copy_scope_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let copied = if !values.is_reference(value)? && values.is_array_like(value)? {
        values.copy_array_value(value)?
    } else {
        values.copy_value(value)?
    };
    context.copy_array_metadata(value, copied);
    if let Err(status) = context.copy_pcntl_foreign_callable(value, copied, values) {
        context.clear_array_metadata(copied);
        let _ = values.release(copied);
        return Err(status);
    }
    Ok(copied)
}

/// Owns an array read even when a legacy alias redirects it to a borrowed variable.
pub(super) fn eval_owned_array_get_result(
    array: RuntimeCellHandle,
    index: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(array)? != EVAL_TAG_OBJECT {
        if let Some(target) = eval_array_reference_key(index, values)?
            .and_then(|key| context.array_element_alias(array, &key).cloned())
        {
            return eval_owned_read(context, values, |context, values, owners| {
                eval_owned_reference_target_value(&target, context, values, owners)
            });
        }
    }
    eval_array_get_result(array, index, context, values)
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

/// Detaches an owned reference read and releases intermediate storage on success or failure.
fn eval_owned_read<V: RuntimeValueOps>(
    context: &mut ElephcEvalContext,
    values: &mut V,
    read: impl FnOnce(&mut ElephcEvalContext, &mut V, &mut Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut owners = Vec::new();
    let result = read(context, values, &mut owners).and_then(|value| {
        if values.is_reference(value)? {
            owners.push(value);
            values.copy_value(value)
        } else {
            Ok(value)
        }
    });
    let mut cleanup = Ok(());
    for value in owners.into_iter().rev() {
        if let Err(status) = eval_release_value(context, values, value) { cleanup = Err(status); }
    }
    match (result, cleanup) {
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Err(status), _) => Err(status),
        (Ok(value), Ok(())) => Ok(value),
    }
}
