//! Purpose:
//! Materializes original key-producing values with balanced source-key and conversion leases.
//!
//! Called from:
//! - Array fill-keys, combine and column adapters.
//!
//! Key details:
//! - Shared exact-key policy runs before a legacy builtin string conversion.
//! - Result builders own insertion leases and abandon partial arrays on errors.

use super::super::super::*;

/// Reads one source value while retiring its independent iteration-key handle.
pub(super) fn value_at(array: RuntimeCellHandle, position: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = values.array_iter_key(array, position)?;
    let result = values.array_get(array, key).and_then(|value| promote_borrowed_result(value, values));
    let released = values.release(key);
    match (result, released) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => { let _ = values.release(value); Err(status) }
    }
}

/// Selects the int|string values that flip/count accept, retiring skipped values.
pub(super) fn countable_key_at(array: RuntimeCellHandle, position: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let value = value_at(array, position, values)?;
    match values.type_tag(value) {
        Ok(EVAL_TAG_INT | EVAL_TAG_STRING) => Ok(Some(value)),
        Ok(_) => { values.release(value)?; Ok(None) }
        Err(status) => { let _ = values.release(value); Err(status) }
    }
}

/// Preserves a key's original kind in exact mode and its builtin-specific legacy conversion otherwise.
pub(super) fn key_at(array: RuntimeCellHandle, position: usize, stringify: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let key = value_at(array, position, values)?;
    if let Err(status) = checked_array_keys::require_key(key, false, context, values) {
        let _ = eval_release_value(context, values, key); return Err(status);
    }
    if !stringify || context.semantic_profile().contains(elephc_semantics::StrictFeature::ArrayKeys) { return Ok(key); }
    let result = values.cast_string(key).and_then(|value| promote_borrowed_result(value, values));
    let released = eval_release_value(context, values, key);
    match (result, released) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => { let _ = eval_release_value(context, values, value); Err(status) }
    }
}
