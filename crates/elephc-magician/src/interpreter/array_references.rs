//! Purpose:
//! Resolves eval array references into owned values for native encoding-list readers.
//!
//! Called from:
//! - The eval array-reference FFI adapter and ownership-aware property getters.
//!
//! Key details:
//! - The original boxed array handle is an identity token, never dereferenced here.
//! - Intermediate owners are returned to a protected native cleanup callback.
//! - No PHP destructor runs while this module holds the eval context borrow.

use super::*;

/// Reads one persistent reference with an owned result and defers intermediate releases.
pub(in crate::interpreter) fn eval_owned_reference_target_value(
    target: &EvalReferenceTarget,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owners: &mut Vec<RuntimeCellHandle>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match target {
        EvalReferenceTarget::Variable { scope, name } => {
            let scope = unsafe { scope.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
            match visible_scope_cell(context, scope, name) {
                Some(value) => values.retain(value),
                None => values.null(),
            }
        },
        EvalReferenceTarget::Cell { cell } => values.retain(*cell),
        EvalReferenceTarget::ArrayElement { scope, array_name, index } => {
            let scope = unsafe { scope.as_mut() }.ok_or(EvalStatus::RuntimeFatal)?;
            let array = match visible_scope_cell(context, scope, array_name) {
                Some(value) => values.retain(value)?,
                None => values.null()?,
            };
            owners.push(array);
            values.array_get(array, *index)
        },
        EvalReferenceTarget::NestedArrayElement { array_target, index } => {
            let array = eval_owned_reference_target_value(array_target, context, values, owners)?;
            owners.push(array);
            values.array_get(array, *index)
        },
        EvalReferenceTarget::ObjectProperty { object, property, access_scope } => {
            let previous = context.replace_execution_scope(access_scope.clone());
            let result = eval_property_get_result_with_ownership(*object, property, context, values, Some(owners));
            context.replace_execution_scope(previous);
            result
        },
        EvalReferenceTarget::StaticProperty { class_name, property, access_scope } => {
            let previous = context.replace_execution_scope(access_scope.clone());
            let result = eval_static_property_get_result_with_ownership(class_name, property, context, values, Some(owners));
            context.replace_execution_scope(previous);
            result
        },
        EvalReferenceTarget::InvokerSlot { .. } => eval_reference_target_value(target, context, values),
    }
}

/// Finds a reference by its original array identity and resolves it at the current read point.
pub(crate) fn read_owned_array_reference(
    original: RuntimeCellHandle,
    key: EvalArrayReferenceKey,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
    owners: &mut Vec<RuntimeCellHandle>,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(target) = context.array_element_alias(original, &key).cloned() else { return Ok(None); };
    eval_owned_reference_target_value(&target, context, values, owners).map(Some)
}
