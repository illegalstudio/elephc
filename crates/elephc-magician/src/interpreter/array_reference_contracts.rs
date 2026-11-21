//! Purpose:
//! Validates array-owned reference element constraints before canonical referent mutation.
//!
//! Called from:
//! - Interpreter reference replacement and the existing native reference validation bridge.
//!
//! Key details:
//! - Allocation tokens and host liveness exclude retired array owners.
//! - Every source value is pinned before fallible validation, without borrowing context maps.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Checks each live array association against the original proposed referent without publishing it.
pub(super) fn validate(reference: RuntimeCellHandle, candidate: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().checked_property_references() { return Ok(()); }
    let sources = context.array_reference_sources();
    let owners = sources.iter().map(|_| RuntimeOwnerSlot::new(
        RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())).collect::<Vec<_>>();
    with_runtime_owners(&owners, || {
        let checked = (|| {
            for (source, owner) in sources.iter().zip(&owners) {
                if values.reference_source_is_live(source.array) {
                    owner.initialize(values.retain_value_cell(source.array)?);
                }
            }
            for (source, owner) in sources.iter().zip(&owners) {
                if owner.borrowed().is_null() { continue; }
                for contract in &source.contracts {
                    if generic_value_accepts_with_reference(&crate::eval_ir::generic_type(contract), owner.borrowed(),
                        reference, candidate, context, values)? == Some(false) {
                        let message = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
                            "--strict-array-types: reference write violates array contents"
                        } else { "Reference write violates declared array contents" };
                        return eval_throw_type_error(message, context, values);
                    }
                }
            }
            Ok(())
        })();
        finish_eval_void_arguments(checked, owners.iter().map(RuntimeOwnerSlot::take), context, values)
    })
}

/// Checks a proposed owner while every occurrence of its destination refers to the proposed value.
pub(super) fn validate_new_owner(reference: RuntimeCellHandle, candidate: RuntimeCellHandle,
    contracts: &[EvalParameterType], context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().checked_property_references()
        || !matches!(values.type_tag(candidate)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(()); }
    for contract in contracts {
        if generic_value_accepts_with_reference(&crate::eval_ir::generic_type(contract), candidate,
            reference, candidate, context, values)? == Some(false) {
            let message = if context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) {
                "--strict-array-types: recursive inferred contents require an explicit mixed contract"
            } else { "Reference write violates a recursive array declaration" };
            return eval_throw_type_error(message, context, values);
        }
    }
    Ok(())
}
