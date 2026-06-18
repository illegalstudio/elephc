//! Purpose:
//! array_pop, array_shift, array_push, and array_unshift replacement helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Returns the value produced by `array_pop()` / `array_shift()` without mutating the source.
pub(in crate::interpreter) fn eval_array_pop_shift_value_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    if len == 0 {
        return values.null();
    }
    let position = match name {
        "array_pop" => len - 1,
        "array_shift" => 0,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let key = values.array_iter_key(array, position)?;
    values.array_get(array, key)
}

/// Builds the return value plus replacement array for direct pop/shift write-back.
pub(in crate::interpreter) fn eval_array_pop_shift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let len = values.array_len(array)?;
    let tag = values.type_tag(array)?;
    if len == 0 {
        let replacement = match tag {
            EVAL_TAG_ARRAY => values.array_new(0)?,
            EVAL_TAG_ASSOC => values.assoc_new(0)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        return Ok((values.null()?, replacement));
    }

    let removed_position = match name {
        "array_pop" => len - 1,
        "array_shift" => 0,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let removed_key = values.array_iter_key(array, removed_position)?;
    let removed_value = values.array_get(array, removed_key)?;
    let replacement = match tag {
        EVAL_TAG_ARRAY => {
            eval_array_pop_shift_indexed_replacement(array, removed_position, len, values)?
        }
        EVAL_TAG_ASSOC => {
            eval_array_pop_shift_assoc_replacement(name, array, removed_position, len, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    Ok((removed_value, replacement))
}

/// Rebuilds an indexed array after removing one position and reindexing values.
pub(in crate::interpreter) fn eval_array_pop_shift_indexed_replacement(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(len.saturating_sub(1))?;
    let mut target = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let target_key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after pop/shift, preserving PHP key behavior.
pub(in crate::interpreter) fn eval_array_pop_shift_assoc_replacement(
    name: &str,
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "array_shift"
        && eval_array_remaining_keys_are_int(array, removed_position, len, values)?
    {
        return eval_array_pop_shift_indexed_replacement(array, removed_position, len, values);
    }

    let mut result = values.assoc_new(len.saturating_sub(1))?;
    let mut next_int_key = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if name == "array_shift" && values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Returns true when every remaining key is an integer after removing one element.
pub(in crate::interpreter) fn eval_array_remaining_keys_are_int(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns the resulting element count for by-value push/unshift dynamic calls.
pub(in crate::interpreter) fn eval_array_push_unshift_count_result(
    array: RuntimeCellHandle,
    inserted_len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let total = values
        .array_len(array)?
        .checked_add(inserted_len)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let total = i64::try_from(total).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(total)
}

/// Builds the replacement array for direct push/unshift write-back.
pub(in crate::interpreter) fn eval_array_push_unshift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (name, values.type_tag(array)?) {
        ("array_push", EVAL_TAG_ARRAY) => {
            eval_array_push_indexed_replacement(array, inserted, values)
        }
        ("array_push", EVAL_TAG_ASSOC) => {
            eval_array_push_assoc_replacement(array, inserted, values)
        }
        ("array_unshift", EVAL_TAG_ARRAY) => {
            eval_array_unshift_indexed_replacement(array, inserted, values)
        }
        ("array_unshift", EVAL_TAG_ASSOC) => {
            eval_array_unshift_assoc_replacement(array, inserted, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Rebuilds an indexed array after appending values.
pub(in crate::interpreter) fn eval_array_push_indexed_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len.saturating_add(inserted.len()))?;
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let target_key =
            values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, target_key, value)?;
    }
    for (offset, value) in inserted.iter().copied().enumerate() {
        let position = len.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?;
        let key = values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after appending values at PHP's next integer keys.
pub(in crate::interpreter) fn eval_array_push_assoc_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.assoc_new(len.saturating_add(inserted.len()))?;
    let mut next_key = 0_i64;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? == EVAL_TAG_INT {
            next_key = next_key.max(eval_int_value(key, values)?.saturating_add(1));
        }
        let value = values.array_get(array, key)?;
        result = values.array_set(result, key, value)?;
    }
    for value in inserted.iter().copied() {
        let key = values.int(next_key)?;
        next_key = next_key.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an indexed array after prepending values and reindexing the original tail.
pub(in crate::interpreter) fn eval_array_unshift_indexed_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len.saturating_add(inserted.len()))?;
    let mut target = 0_i64;
    for value in inserted.iter().copied() {
        let key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let key = values.int(target)?;
        target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Rebuilds an associative array after prepending values and reindexing integer keys.
pub(in crate::interpreter) fn eval_array_unshift_assoc_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if eval_array_keys_are_int(array, len, values)? {
        return eval_array_unshift_indexed_replacement(array, inserted, values);
    }

    let mut result = values.assoc_new(len.saturating_add(inserted.len()))?;
    let mut next_int_key = 0_i64;
    for value in inserted.iter().copied() {
        let key = values.int(next_int_key)?;
        next_int_key = next_int_key
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, key, value)?;
    }
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
            let key = values.int(next_int_key)?;
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            key
        } else {
            source_key
        };
        let value = values.array_get(array, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Returns true when every key in the array is integer-shaped.
pub(in crate::interpreter) fn eval_array_keys_are_int(
    array: RuntimeCellHandle,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}
