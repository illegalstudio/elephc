//! Purpose:
//! Declarative eval registry entry for `array_splice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{
    EvalArrayBuilder, with_collection_call, with_collection_cells, with_collection_result,
};

eval_builtin! {
    contract: "array_splice",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_splice` array mutator.
pub(in crate::interpreter) fn eval_array_splice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match evaluated_args {
        [array, offset] => eval_array_splice_value_result(*array, *offset, None, values)?,
        [array, offset, length] => eval_array_splice_value_result(*array, *offset, Some(*length), values)?,
        [array, offset, length, _replacement] => eval_array_splice_value_result(*array, *offset, Some(*length), values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.warning("array_splice(): Argument #1 ($array) must be passed by reference, value given")?;
    Ok(result)
}

/// Evaluates direct by-reference `array_splice()` calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_splice_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_collection_call(context, values, |context, values, owners, output| {
        let (array, target, offset, length, replacement_arg) =
            eval_array_splice_direct_args(args, context, scope, values, owners)?;
        let (removed, replacement) =
            eval_array_splice_removed_and_replacement(array, offset, length, replacement_arg, values)?;
        output.replace(Some(removed));
        write_back_owned_ref_target(&target, replacement, context, values)
    })
}

/// Evaluates and binds direct `array_splice()` arguments while preserving source order.
pub(in crate::interpreter) fn eval_array_splice_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    owners: &crate::runtime_hooks::owner_frames::StagedRuntimeOwners,
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
                array = Some(super::mutation::eval_array_mutation_lvalue_arg(
                    arg, context, scope, values, owners,
                )?);
            }
            "offset" => {
                if offset.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                offset = Some(owners.stage(eval_owned_expr(arg.value(), context, scope, values)?));
            }
            "length" => {
                if length.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                length = Some(owners.stage(eval_owned_expr(arg.value(), context, scope, values)?));
            }
            "replacement" => {
                if replacement.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                replacement = Some(owners.stage(eval_owned_expr(arg.value(), context, scope, values)?));
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (array, target) = array.ok_or(EvalStatus::RuntimeFatal)?;
    let offset = offset.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, target, offset, length, replacement))
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
    let mut removed = None;
    let replacement = with_collection_result(values, |values, rebuilt| {
        let original = crate::runtime_hooks::owner_frames::RuntimeOwnerSlot::new(
            eval_array_splice_removed(array, start, end, values)?,
        );
        crate::runtime_hooks::owner_frames::with_runtime_owners(&original, || {
            let outcome = with_collection_cells(values, |values, owners| {
                let inserted = eval_array_splice_insert_values(replacement, values, owners)?;
                let replacement = eval_array_splice_replacement(array, start, end, &inserted, values)?;
                rebuilt.replace(Some(replacement));
                Ok(())
            });
            removed = Some(original.finish(outcome, |cell| values.release(cell))?);
            Ok(())
        })
    })?;
    Ok((removed.expect("successful splice"), replacement))
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
    let start = super::array_slice::eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            super::array_slice::eval_slice_end(len, start, eval_int_value(length, values)?)?
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
        let mut result = EvalArrayBuilder::indexed(values, len)?;
        let mut target = 0_i64;
        for position in start..end {
            result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(array, position)?);
                let value = owners.stage(values.array_get(array, key)?);
                let target_key = owners.stage(values.int(target)?);
                target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
                Ok(Some((target_key, value)))
            })?;
        }
        return Ok(result.finish());
    }

    let mut result = EvalArrayBuilder::assoc(values, len)?;
    let mut next_int_key = 0_i64;
    for position in start..end {
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
                let key = owners.stage(values.int(next_int_key)?);
                next_int_key = next_int_key
                    .checked_add(1)
                    .ok_or(EvalStatus::RuntimeFatal)?;
                key
            } else {
                source_key
            };
            let value = owners.stage(values.array_get(array, source_key)?);
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}

/// Expands the optional `array_splice()` replacement value into inserted values.
pub(in crate::interpreter) fn eval_array_splice_insert_values(
    replacement: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
    owners: &crate::runtime_hooks::owner_frames::StagedRuntimeOwners,
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
        let key = owners.stage(values.array_iter_key(replacement, position)?);
        inserted.push(owners.stage(values.array_get(replacement, key)?));
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
        let mut result = EvalArrayBuilder::indexed(values, new_len)?;
        let mut target = 0_i64;
        for position in 0..start {
            result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(array, position)?);
                let value = owners.stage(values.array_get(array, key)?);
                let target_key = owners.stage(values.int(target)?);
                target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
                Ok(Some((target_key, value)))
            })?;
        }
        for value in inserted {
            result.update(|values, owners, _| {
                let target_key = owners.stage(values.int(target)?);
                target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
                Ok(Some((target_key, *value)))
            })?;
        }
        for position in end..len {
            result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(array, position)?);
                let value = owners.stage(values.array_get(array, key)?);
                let target_key = owners.stage(values.int(target)?);
                target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
                Ok(Some((target_key, value)))
            })?;
        }
        return Ok(result.finish());
    }

    let mut result = EvalArrayBuilder::assoc(values, new_len)?;
    let mut next_int_key = 0_i64;
    for position in 0..start {
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
                let key = owners.stage(values.int(next_int_key)?);
                next_int_key = next_int_key
                    .checked_add(1)
                    .ok_or(EvalStatus::RuntimeFatal)?;
                key
            } else {
                source_key
            };
            let value = owners.stage(values.array_get(array, source_key)?);
            Ok(Some((target_key, value)))
        })?;
    }
    for value in inserted {
        result.update(|values, owners, _| {
            let target_key = owners.stage(values.int(next_int_key)?);
            next_int_key = next_int_key
                .checked_add(1)
                .ok_or(EvalStatus::RuntimeFatal)?;
            Ok(Some((target_key, *value)))
        })?;
    }
    for position in end..len {
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let target_key = if values.type_tag(source_key)? == EVAL_TAG_INT {
                let key = owners.stage(values.int(next_int_key)?);
                next_int_key = next_int_key
                    .checked_add(1)
                    .ok_or(EvalStatus::RuntimeFatal)?;
                key
            } else {
                source_key
            };
            let value = owners.stage(values.array_get(array, source_key)?);
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}

/// Returns true when every key in one source position range is integer-shaped.
pub(in crate::interpreter) fn eval_array_range_keys_are_int(
    array: RuntimeCellHandle,
    start: usize,
    end: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in start..end {
        let integer = with_collection_cells(values, |values, owners| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            Ok(values.type_tag(key)? == EVAL_TAG_INT)
        })?;
        if !integer {
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
    Ok(eval_array_range_keys_are_int(array, 0, start, values)?
        && eval_array_range_keys_are_int(array, end, len, values)?)
}
