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
    eval_reject_null_object_property(&declaring_class, &property, reference, context, values)?;
    let storage_name = eval_instance_property_storage_name(&declaring_class, &property);
    context.bind_dynamic_property_alias(identity, &storage_name,
        EvalReferenceTarget::Cell { cell: reference.borrowed() });
    eval_with_native_property_storage_scope(&object_class_name, &storage_name, context, values,
        |values| values.property_set(object, &storage_name, reference))?;
    context.mark_dynamic_property_initialized(identity, &storage_name);
    Ok(true)
}
