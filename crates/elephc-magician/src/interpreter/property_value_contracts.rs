//! Purpose:
//! Validates eval-owned property candidates before publication or reference binding.
//!
//! Called from:
//! - Instance, static and Reflection writes plus property-reference preparation.
//!
//! Key details:
//! - Coercions uses the shared exact declaration membership predicate.
//! - Permissive non-null object checks preserve their existing diagnostics and owner rules.
//! - Refused candidates leave property storage and initialization state untouched.

use super::*;

/// Publishes a borrowed physical array root on its existing instance binding before old-value retirement.
pub(in crate::interpreter) fn eval_note_instance_property_array_value(identity: u64, storage: &str,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
    context.publish_dynamic_property_array_source(identity, storage, array.then_some(value));
    Ok(())
}

/// Publishes a declaring static slot's borrowed root independently from local array owner metadata.
pub(in crate::interpreter) fn eval_note_static_property_array_value(class: &str, property: &str,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let array = matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC);
    context.publish_static_property_array_source(class, property, array.then_some(value));
    Ok(())
}

/// Copies static declaration and binding origins before a published array payload can change.
pub(in crate::interpreter) fn eval_static_property_array_contracts(class: &str, name: &str,
    context: &ElephcEvalContext,
) -> Result<Vec<EvalParameterType>, EvalStatus> {
    let class = resolve_eval_static_member_class_name(class, context)?;
    let Some((owner, property)) = context.class_property(&class, name) else { return Ok(Vec::new()); };
    Ok(property.property_type().into_iter().chain(context.static_property_array_type(&owner, property.name())).cloned().collect())
}

/// Prepares an untyped default's content origin while the caller owns its initialized value.
pub(in crate::interpreter) fn eval_default_property_array_origin(property: &EvalClassProperty,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalParameterType>, EvalStatus> {
    if property.property_type().is_some() { return Ok(None); }
    let prepared = local_binding_origins::prepare_array_value_origin(None, None, value, property.name(), context, values);
    if prepared.is_err() { let _ = eval_release_value(context, values, value); }
    prepared
}

/// Checks a physical instance binding without retaining its receiver or mutating its content origin.
pub(in crate::interpreter) fn eval_validate_instance_property_array_value(identity: u64, storage: &str,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(origin) = context.dynamic_property_array_type(identity, storage).cloned() {
        validate_generic_local(&origin, storage, value, context, values)?;
    }
    Ok(())
}

/// Validates an eval-declared property before changing its backing slot or a reference alias.
pub(in crate::interpreter) fn eval_validate_property_value(
    owner: &str,
    property: &EvalClassProperty,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(ty) = property.property_type() else { return Ok(()); };
    eval_validate_property_contract(owner, property.name(), ty, value, context, values)
}

/// Checks a borrowed candidate against a physical property's immutable declaration.
pub(in crate::interpreter) fn eval_validate_property_contract(
    owner: &str, property: &str, ty: &EvalParameterType, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    checked_nullability::declared(value, ty, context, values)?;
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions)
        && !eval_declared_value_type_accepts_exact(ty, Some(owner), Some(owner), value, context, values)?
    {
        let expected = eval_null_boundary_type_label(ty, Some(owner), Some(owner), context)?;
        return eval_throw_type_error(&format!(
            "Cannot assign value to property {}::${} of type {expected} (--strict-coercions)",
            owner.trim_start_matches('\\'), property), context, values);
    }
    if !eval_nonnull_object_boundary(ty) || values.type_tag(value)? != EVAL_TAG_NULL {
        return Ok(());
    }
    let expected = eval_null_boundary_type_label(ty, Some(owner), Some(owner), context)?;
    eval_throw_type_error(
        &format!(
            "Cannot assign null to property {}::${} of type {expected}",
            owner.trim_start_matches('\\'),
            property
        ),
        context,
        values,
    )
}

/// Shares static-slot validation with Reflection before a borrowed write can replace its owner.
pub(in crate::interpreter) fn eval_validate_static_property_value(
    class: &str,
    property: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some((owner, metadata)) = context.class_property(class, property) {
        eval_validate_property_value(&owner, &metadata, value, context, values)?;
        if let Some(origin) = context.static_property_array_type(&owner, metadata.name()).cloned() {
            validate_generic_local(&origin, metadata.name(), value, context, values)?;
        }
    }
    Ok(())
}

/// Checks a top-level source before preparation can retarget an already published hidden alias.
pub(in crate::interpreter) fn eval_preflight_property_reference_source(
    owner: &str, property: &EvalClassProperty, source: &str,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if context.current_function().is_some() || scope.reference_target(source).is_some()
        || !property.property_type().is_some_and(|ty| eval_nonnull_object_boundary(ty)
            || context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions)
            || context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability)
                && checked_nullability::non_null_contract(ty)) {
        return Ok(());
    }
    let value = match visible_scope_cell(context, scope, source) {
        Some(value) => value,
        None => {
            // PHP creates an absent source as null even when the property bind is refused.
            let value = values.null()?;
            for previous in set_owned_scope_cell(context, scope, source.to_string(), value, values)? {
                eval_release_value(context, values, previous)?;
            }
            value
        }
    };
    eval_validate_property_value(owner, property, value, context, values)
}
