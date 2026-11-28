//! Purpose:
//! Copies an array entry and its PHP reference association under a selected output key.
//!
//! Called from:
//! - Rooted push, unshift and splice replacement builders.
//!
//! Key details:
//! - Callers retain the source, output and keys across this operation.
//! - The extracted element owns a temporary root until insertion and metadata copy finish.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Preserves persistent references and fallback alias targets while remapping one entry's key.
pub(super) fn copy_entry(source: RuntimeCellHandle, source_key: RuntimeCellHandle,
    output: RuntimeCellHandle, output_key: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let element = RuntimeOwnerSlot::new(values.array_get_preserving_references(source, source_key)?);
    with_runtime_owners(&element, || {
        let result = (|| {
            values.array_set(output, output_key, element.borrowed())?;
            let source_key = eval_array_reference_key(source_key, context.semantic_profile(), values)?;
            let target = source_key.and_then(|key| context.array_element_alias(source, &key).cloned());
            if let Some(target) = target {
                if let Some(key) = eval_array_reference_key(output_key, context.semantic_profile(), values)? {
                    context.bind_array_element_alias(output, key, target);
                }
            }
            Ok(())
        })();
        finish_eval_void_arguments(result, [element.take()], context, values)
    })
}
