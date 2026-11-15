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
            || context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions)) {
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
