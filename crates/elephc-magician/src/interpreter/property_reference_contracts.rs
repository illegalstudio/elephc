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
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(None); }
    let cell = eval_persistent_variable_reference(source, context, scope, values)?
        .ok_or(EvalStatus::UnsupportedConstruct)?;
    Ok(Some(EvalReferenceTarget::Cell { cell: cell.borrowed() }))
}

/// Refuses incompatible property candidates before the underlying cell or old value changes.
pub(in crate::interpreter) fn eval_replace_reference_value(
    reference: RuntimeCellHandle, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_validate_property_reference(reference, value, context, values)?;
    // A reference may escape the eval activation that owns its property's declaration.
    let current = context as *mut ElephcEvalContext;
    for pointer in unsafe { crate::context::live_native_reference_contexts() } {
        if pointer == current { continue; }
        let Some(owner) = (unsafe { pointer.as_mut() }) else { continue; };
        if let Err(status) = eval_validate_property_reference(reference, value, owner, values) {
            if let Some(error) = owner.take_pending_throw() { context.set_pending_throw(error); }
            return Err(status);
        }
    }
    values.reference_replace(reference, value)
}

/// Validates the current declaration set without publishing, retaining or consuming a candidate.
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
