//! Purpose:
//! Publishes escaping parameter reference cells into eval-declared backing properties.
//!
//! Called from:
//! - By-reference object-property writeback after eval function and method calls.
//!
//! Key details:
//! - Runtime property storage retains the cell while context alias metadata borrows it.
//! - The receiver may die after writeback without invalidating escaped references.
//! - Native storage and hooked properties keep their existing value writeback path.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Captures mutable eval storage before later arguments can change or detach its binding.
pub(in crate::interpreter) fn eval_property_reference_for_access(
    object: RuntimeCellHandle, property_name: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) {
        return Ok(None);
    }
    let identity = values.object_identity(object)?;
    let Some(class) = context.dynamic_object_class(identity) else { return Ok(None); };
    let Some((owner, property)) = eval_dynamic_property_for_access(class.name(), property_name, context)
    else { return Ok(None); };
    if property.has_get_hook() || property.has_set_hook() || property.is_virtual() || property.is_readonly() {
        return Ok(None);
    }
    validate_eval_property_write_access(&owner, &property, context)?;
    let value = RuntimeOwnerSlot::new(eval_property_get_result(object, property_name, context, values)?);
    with_runtime_owners(&value, || {
        let result = (|| {
            if values.is_reference(value.borrowed())? { return Ok(value.take()); }
            let reference = RuntimeOwnerSlot::new(values.reference_new(value.borrowed())?);
            with_runtime_owners(&reference, || {
                match bind_escaped_cell(object, property_name, reference.borrowed(), context, values) {
                    Ok(true) => Ok(reference.take()),
                    Ok(false) => finish_synthesized_method_arguments(Err(EvalStatus::RuntimeFatal), [reference.take()], context, values),
                    Err(status) => finish_synthesized_method_arguments(Err(status), [reference.take()], context, values),
                }
            })
        })();
        finish_synthesized_method_arguments(result, [value.take()], context, values).map(Some)
    })
}

/// Promotes an already authorized eval static slot into stable storage for a reference argument.
pub(in crate::interpreter) fn eval_static_property_reference_after_read(
    class_name: &str, property_name: &str, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(None); }
    let class_name = resolve_eval_static_member_class_name(class_name, context)?;
    let Some((owner, property)) = context.class_property(&class_name, property_name) else { return Ok(None); };
    if !property.is_static() { return Ok(None); }
    let Some(current) = context.static_property(&owner, property.name()) else { return Ok(None); };
    if values.is_reference(current)? { return values.retain(current).map(Some); }
    let reference = RuntimeOwnerSlot::new(values.reference_new(current)?);
    with_runtime_owners(&reference, || {
        let result = (|| {
            let retained = values.retain(reference.borrowed())?;
            context.bind_static_property_alias(&owner, property.name(),
                EvalReferenceTarget::Cell { cell: reference.borrowed() });
            store_static_property_value(&owner, property.name(), retained, context, values)?;
            Ok(reference.take())
        })();
        finish_synthesized_method_arguments(result, [reference.take()], context, values).map(Some)
    })
}

/// Links an escaped reference to ordinary eval storage while preserving property access checks.
pub(super) fn bind_escaped_cell(
    object: RuntimeCellHandle, property_name: &str, reference: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let identity = values.object_identity(object)?;
    let Some(class) = context.dynamic_object_class(identity) else { return Ok(false); };
    let object_class_name = class.name().to_string();
    let Some((declaring_class, property)) =
        eval_dynamic_property_for_access(&object_class_name, property_name, context)
    else { return Ok(false); };
    if property.has_get_hook() || property.has_set_hook() || property.is_virtual() { return Ok(false); }
    validate_eval_property_write_access(&declaring_class, &property, context)?;
    if property.is_readonly() { return Err(EvalStatus::RuntimeFatal); }
    eval_validate_property_value(&declaring_class, &property, reference, context, values)?;
    let storage_name = eval_instance_property_storage_name(&declaring_class, &property);
    context.bind_dynamic_property_alias(identity, &storage_name,
        EvalReferenceTarget::Cell { cell: reference.borrowed() });
    eval_with_native_property_storage_scope(&object_class_name, &storage_name, context, values,
        |values| values.property_set(object, &storage_name, reference))?;
    context.mark_dynamic_property_initialized(identity, &storage_name);
    Ok(true)
}
