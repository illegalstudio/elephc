//! Purpose:
//! Validates live eval property constraints at canonical reference publication.
//!
//! Called from:
//! - Property binding and every interpreter-owned persistent reference replacement.
//!
//! Key details:
//! - Source associations are queried at the write, so receiver retirement releases constraints.
//! - Source variables use persistent cells before aliases or captures can escape their frame.
//! - This adapter borrows candidates; ordinary publication keeps its existing owner transfers.

use super::*;

/// Leases persistent property sources in every profile, requiring that capability when checked.
pub(in crate::interpreter) fn owned_property_reference_source(
    source: &str, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    if let Some(cell) = eval_persistent_variable_reference(source, context, scope, values)? {
        return Ok(Some(EvalReferenceTarget::Cell { cell: cell.borrowed() }));
    }
    if context.semantic_profile().checked_property_references() {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    Ok(None)
}

/// Refuses incompatible property candidates before the underlying cell or old value changes.
pub(in crate::interpreter) fn eval_replace_reference_value(
    reference: RuntimeCellHandle, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context.invalidate_shared_source_views(reference);
    if values.coordinates_property_reference_candidates() {
        return values.reference_replace(reference, value);
    }
    array_reference_contracts::validate(reference, value, context, values)?;
    let mut prepared = prepare_property_reference_candidates(reference, value, context, values)?;
    let current = context as *mut ElephcEvalContext;
    for pointer in unsafe { crate::context::live_native_reference_contexts() } {
        if pointer == current { continue; }
        let Some(owner) = (unsafe { pointer.as_mut() }) else { continue; };
        owner.invalidate_shared_source_views(reference);
        array_reference_contracts::validate(reference, value, owner, values)?;
        prepared.extend(prepare_property_reference_candidates(reference, value, owner, values)?);
    }
    if let Some(candidate) = coerce_property_reference_candidates(prepared, value, context, values)? {
        let result = values.reference_replace(reference, candidate);
        return finish_synthesized_method_arguments(result, [candidate], context, values);
    }
    values.reference_replace(reference, value)
}

/// Validates the current declaration set without publishing, retaining or consuming a candidate.
#[cfg(not(test))]
pub(crate) fn eval_validate_property_reference(
    reference: RuntimeCellHandle, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    context.invalidate_shared_source_views(reference);
    array_reference_contracts::validate(reference, value, context, values)?;
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
        let contracts = context.local_reference_types(reference);
        for declared in &contracts { validate_generic_local(declared, "reference", value, context, values)?; }
        array_reference_contracts::validate_new_owner(reference, value, &contracts, context, values)?;
    }
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Locals) {
        for declared in context.local_reference_types(reference) {
            if !eval_declared_value_type_accepts_exact(&declared, None, None, value, context, values)? {
                return eval_throw_type_error("Cannot change a strict local binding's value type", context, values);
            }
        }
    }
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions)
        && !context.semantic_profile().contains(elephc_semantics::StrictFeature::Nullability) { return Ok(()); }
    for source in context.property_reference_types(reference) {
        eval_validate_property_contract(&source.owner, &source.property, &source.declared,
            value, context, values)?;
    }
    Ok(())
}

/// Coerces a prospective property's source through the shared coordinator before publishing its alias.
pub(in crate::interpreter) fn prepare_property_reference_admission(owner: &str,property: &EvalClassProperty,
    target: &EvalReferenceTarget,context: &mut ElephcEvalContext,values: &mut impl RuntimeValueOps,
) -> Result<(),EvalStatus> {
    if !context.semantic_profile().checked_property_references() {return Ok(());}
    let Some(declared)=property.property_type() else {return Ok(());};
    let EvalReferenceTarget::Cell {cell}=target else {return Err(EvalStatus::UnsupportedConstruct);};
    if eval_declared_value_type_accepts_exact(declared,Some(owner),Some(owner),*cell,context,values)? {
        return Ok(());
    }
    let original=values.copy_value(*cell)?;
    with_eval_value_lease(original,context,values,|original,context,values| {
        context.with_property_reference_admission(*cell,owner,property.name(),declared,|context| {
            let previous=eval_replace_reference_value(*cell,original,context,values)?;
            eval_release_value(context,values,previous)
        })
    })
}
