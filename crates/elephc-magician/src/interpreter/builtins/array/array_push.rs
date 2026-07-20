//! Purpose:
//! Declarative eval registry entry for `array_push`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_push",
    area: Array,
    params: [array: by_ref],
    variadic: values,
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_push` array mutator.
pub(in crate::interpreter) fn eval_array_push_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((array, inserted)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("array_push", values)?;
    eval_array_push_unshift_count_result(*array, inserted.len(), values)
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

/// Evaluates direct by-reference `array_push()` / `array_unshift()` calls and writes back the array.
pub(in crate::interpreter) fn eval_array_push_unshift_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 || !eval_call_args_are_plain_positional(args) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = super::mutation::eval_array_mutation_lvalue_arg(&args[0], context, scope, values)?;
    let mut inserted = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        inserted.push(eval_expr(arg.value(), context, scope, values)?);
    }

    let replacement = eval_array_push_unshift_replacement(name, array, &inserted, values)?;
    let result = eval_array_push_unshift_count_result(array, inserted.len(), values)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}
