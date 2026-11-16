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

/// Gives checked property sources stable cells without adding permanent local type constraints.
pub(in crate::interpreter) fn checked_property_reference_source(
    source: &str, context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    if !context.semantic_profile().checked_property_references() { return Ok(None); }
    let cell = eval_persistent_variable_reference(source, context, scope, values)?
        .ok_or(EvalStatus::UnsupportedConstruct)?;
    Ok(Some(EvalReferenceTarget::Cell { cell: cell.borrowed() }))
}

/// Refuses incompatible property candidates before the underlying cell or old value changes.
pub(in crate::interpreter) fn eval_replace_reference_value(
    reference: RuntimeCellHandle, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.coordinates_property_reference_candidates() {
        return values.reference_replace(reference, value);
    }
    let mut prepared = prepare_property_reference_candidates(reference, value, context, values)?;
    let current = context as *mut ElephcEvalContext;
    for pointer in unsafe { crate::context::live_native_reference_contexts() } {
        if pointer == current { continue; }
        let Some(owner) = (unsafe { pointer.as_mut() }) else { continue; };
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
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(()); }
    for source in context.property_reference_types(reference) {
        eval_validate_property_contract(&source.owner, &source.property, &source.declared,
            value, context, values)?;
    }
    Ok(())
}
