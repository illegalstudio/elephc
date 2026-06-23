//! Purpose:
//! array_splice argument handling and replacement construction helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;
use super::*;

/// Evaluates direct by-reference `array_splice()` calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_splice_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array_name, offset, length, replacement_arg) =
        eval_array_splice_direct_args(args, context, scope, values)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let (removed, replacement) =
        eval_array_splice_removed_and_replacement(array, offset, length, replacement_arg, values)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(removed)
}

/// Evaluates and binds direct `array_splice()` arguments while preserving source order.
pub(in crate::interpreter) fn eval_array_splice_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySpliceDirectArgs, EvalStatus> {
    let mut array = None;
    let mut offset = None;
    let mut length = None;
    let mut replacement = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "array",
                1 => "offset",
                2 => "length",
                3 => "replacement",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "array" => {
                if array.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let EvalExpr::LoadVar(name) = arg.value() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                array = Some(name.clone());
            }
            "offset" => {
                if offset.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                offset = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "length" => {
                if length.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                length = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "replacement" => {
                if replacement.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                replacement = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let array = array.ok_or(EvalStatus::RuntimeFatal)?;
    let offset = offset.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, offset, length, replacement))
}

/// Returns the removed elements that `array_splice()` would produce without mutating the source.
pub(in crate::interpreter) fn eval_array_splice_value_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    eval_array_splice_removed(array, start, end, values)
}

/// Builds both removed and replacement arrays for direct `array_splice()` write-back.
pub(in crate::interpreter) fn eval_array_splice_removed_and_replacement(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    replacement: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    let removed = eval_array_splice_removed(array, start, end, values)?;
    let inserted = eval_array_splice_insert_values(replacement, values)?;
    let replacement = eval_array_splice_replacement(array, start, end, &inserted, values)?;
    Ok((removed, replacement))
}

/// Converts splice offset and length cells into bounded source positions.
pub(in crate::interpreter) fn eval_array_splice_bounds(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(usize, usize), EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };
    Ok((start, end))
}

/// Builds the reindexed/string-key-preserving removed array returned by `array_splice()`.
pub(in crate::interpreter) fn eval_array_splice_removed(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = end.saturating_sub(start);
    if eval_array_range_keys_are_int(array, start, end, values)? {
        let mut result = values.array_new(len)?;
        let mut target = 0_i64;
        for position in start..end {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        return Ok(result);
    }

    let mut result = values.assoc_new(len)?;
    let mut next_int_key = 0_i64;
    for position in start..end {
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

/// Expands the optional `array_splice()` replacement value into inserted values.
pub(in crate::interpreter) fn eval_array_splice_insert_values(
    replacement: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let Some(replacement) = replacement else {
        return Ok(Vec::new());
    };
    if !matches!(
        values.type_tag(replacement)?,
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC
    ) {
        return Ok(vec![replacement]);
    }

    let len = values.array_len(replacement)?;
    let mut inserted = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(replacement, position)?;
        inserted.push(values.array_get(replacement, key)?);
    }
    Ok(inserted)
}

/// Builds the source replacement after removing the requested splice range.
pub(in crate::interpreter) fn eval_array_splice_replacement(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    inserted: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let new_len = len
        .saturating_sub(end.saturating_sub(start))
        .checked_add(inserted.len())
        .ok_or(EvalStatus::RuntimeFatal)?;
    if eval_array_splice_remaining_keys_are_int(array, start, end, len, values)? {
        let mut result = values.array_new(new_len)?;
        let mut target = 0_i64;
        for position in 0..start {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        for value in inserted {
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, *value)?;
        }
        for position in end..len {
            let key = values.array_iter_key(array, position)?;
            let value = values.array_get(array, key)?;
            let target_key = values.int(target)?;
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            result = values.array_set(result, target_key, value)?;
        }
        return Ok(result);
    }

    let mut result = values.assoc_new(new_len)?;
    let mut next_int_key = 0_i64;
    for position in 0..start {
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
    for value in inserted {
        let target_key = values.int(next_int_key)?;
        next_int_key = next_int_key
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, *value)?;
    }
    for position in end..len {
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

/// Returns true when every key in one source position range is integer-shaped.
pub(in crate::interpreter) fn eval_array_range_keys_are_int(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in start..end {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns true when every key outside the removed splice range is integer-shaped.
pub(in crate::interpreter) fn eval_array_splice_remaining_keys_are_int(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        if (start..end).contains(&position) {
            continue;
        }
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            return Ok(false);
        }
    }
    Ok(true)
}
