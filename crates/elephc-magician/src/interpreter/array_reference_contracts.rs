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
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayTypes) { return Ok(()); }
    let sources = context.array_reference_sources(reference);
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
                let key = RuntimeOwnerSlot::new(match &source.key {
                    EvalArrayReferenceKey::Int(key) => values.int(*key)?,
                    EvalArrayReferenceKey::String(key) => values.string_bytes_value(key)?,
                });
                with_runtime_owners(&key, || {
                    let result = (|| {
                        for contract in &source.contracts {
                            validate_generic_local_element(Some(contract), owner.borrowed(), key.borrowed(), candidate, context, values)?;
                        }
                        Ok(())
                    })();
                    finish_eval_void_arguments(result, [key.take()], context, values)
                })?;
            }
            Ok(())
        })();
        finish_eval_void_arguments(checked, owners.iter().map(RuntimeOwnerSlot::take), context, values)
    })
}
