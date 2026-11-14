//! Purpose:
//! Builds checked settype object results without changing the original reference receiver.
//!
//! Called from:
//! - The eval settype target dispatcher after source-order argument evaluation.
//!
//! Key details:
//! - Existing objects retain identity; new stdClass writes cannot invoke user property hooks.
//! - Byte names and insertion-order values stay paired, including numeric-string keys.

use super::*;
use crate::runtime_hooks::owner_frames::{with_runtime_owners, RuntimeOwnerSlot};

/// Produces an independent object owner before the caller publishes any reference mutation.
pub(super) fn object(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_OBJECT { return values.retain(value); }
    let object = RuntimeOwnerSlot::new(values.new_object("stdClass")?);
    with_runtime_owners(&object, || {
        let result = (|| {
            if values.is_array_like(value)? {
                for position in 0..values.array_len(value)? {
                    let updated = property(object.borrowed(), value, position, context, values)?;
                    values.release(updated)?;
                }
            } else if !values.is_null(value)? {
                values.property_set_bytes(object.borrowed(), b"scalar", value)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(object.take()),
            Err(status) => { let _ = eval_release_value(context, values, object.take()); Err(status) }
        }
    })
}

/// Writes one byte-named property while retiring every temporary key, text and value on failure.
fn property(object: RuntimeCellHandle, array: RuntimeCellHandle, position: usize,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = RuntimeOwnerSlot::new(values.array_iter_key(array, position)?);
    let result = with_runtime_owners(&key, || {
        let text = RuntimeOwnerSlot::new(values.cast_string(key.borrowed())?);
        let result = with_runtime_owners(&text, || {
            let bytes = values.string_bytes(text.borrowed())?;
            let element = RuntimeOwnerSlot::new(values.array_iter_value(array, position)?);
            let result = with_runtime_owners(&element, ||
                values.property_set_bytes(object, &bytes, element.borrowed()).map(|()| object));
            finish_synthesized_method_arguments(result, [element.take()], context, values)
        });
        finish_synthesized_method_arguments(result, [text.take()], context, values)
    });
    finish_synthesized_method_arguments(result, [key.take()], context, values)
}
