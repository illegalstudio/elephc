//! Purpose:
//! Materializes temporary runtime keys from independently owned reference metadata.
//!
//! Called from:
//! - Persistent reference reads and writeback after the original call operands have retired.
//!
//! Key details:
//! - Metadata owns integer identities or exact byte strings, never runtime key cells.
//! - Every materialized key is rooted and released on success and failure.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Copies a selected argument key without diagnosing its completed float conversion again.
pub(super) fn snapshot(
    key: RuntimeCellHandle, profile: elephc_semantics::SemanticProfile,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArrayReferenceKey, EvalStatus> {
    if values.type_tag(key)? == EVAL_TAG_FLOAT {
        let integer = values.cast_int(key)?;
        let result = eval_array_reference_key(integer, profile, values);
        let released = values.release(integer);
        return result.and_then(|key| released.map(|()| key))?
            .ok_or(EvalStatus::UnsupportedConstruct);
    }
    eval_array_reference_key(key, profile, values)?.ok_or(EvalStatus::UnsupportedConstruct)
}

/// Creates one owned runtime key without repeating normalization or key diagnostics.
pub(super) fn materialize(key: &EvalArrayReferenceKey, values: &mut impl RuntimeValueOps)
    -> Result<RuntimeCellHandle, EvalStatus>
{
    match key {
        EvalArrayReferenceKey::Int(key) => values.int(*key),
        EvalArrayReferenceKey::String(bytes) => values.string_bytes_value(bytes),
    }
}

/// Consumes an owned parent lookup, or borrows an existing parent, through a keyed read.
pub(super) fn read(
    array: RuntimeCellHandle, key: &EvalArrayReferenceKey,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = RuntimeOwnerSlot::new(array);
    with_runtime_owners(&array, || {
        let key = match materialize(key, values) {
            Ok(key) => RuntimeOwnerSlot::new(key),
            Err(status) => return finish_synthesized_method_arguments(Err(status), [array.take()], context, values),
        };
        with_runtime_owners(&key, || {
            let result = values.array_get(array.borrowed(), key.borrowed());
            finish_synthesized_method_arguments(result, [key.take(), array.take()], context, values)
        })
    })
}

/// Borrows a temporary key through validation and mutation, consuming its owner on every exit.
pub(super) fn with_key<V: RuntimeValueOps>(
    key: &EvalArrayReferenceKey, context: &mut ElephcEvalContext, values: &mut V,
    write: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let key = RuntimeOwnerSlot::new(materialize(key, values)?);
    with_runtime_owners(&key, || {
        let result = write(key.borrowed(), context, values);
        let released = eval_release_value(context, values, key.take());
        result.and(released)
    })
}
