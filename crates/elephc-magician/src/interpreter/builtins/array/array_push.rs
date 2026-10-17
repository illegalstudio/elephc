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
    contract: "array_push",
    area: Array,
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
///
/// Associative `array_push` reports a saturated cursor separately so a free
/// `PHP_INT_MAX` key is inserted and only the following value is rejected.
pub(in crate::interpreter) fn eval_array_push_unshift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<ArrayPushReplacement, EvalStatus> {
    let replacement = match (name, values.type_tag(array)?) {
        ("array_push", EVAL_TAG_ARRAY) => {
            eval_array_push_indexed_replacement(array, inserted, values)?
        }
        ("array_push", EVAL_TAG_ASSOC) => {
            return eval_array_push_assoc_replacement(array, inserted, context, values);
        }
        ("array_unshift", EVAL_TAG_ARRAY) => {
            eval_array_unshift_indexed_replacement(array, inserted, values)?
        }
        ("array_unshift", EVAL_TAG_ASSOC) => {
            eval_array_unshift_assoc_replacement(array, inserted, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    Ok(ArrayPushReplacement { replacement: Some(replacement), overflow: false })
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

/// Message PHP raises when an append's next integer key is already occupied.
pub(in crate::interpreter) const ARRAY_NEXT_ELEMENT_OCCUPIED: &str =
    "Cannot add element to the array as the next element is already occupied";

/// A rebuilt `array_push` / `array_unshift` array, plus whether a later value could not be inserted.
///
/// `replacement` is `None` when nothing was inserted, so the caller leaves the original array
/// alone. A `Some` replacement is written back even when `overflow` is set: PHP inserts every
/// value that fits, then throws, and `unset` does not rewind a saturated cursor.
pub(in crate::interpreter) struct ArrayPushReplacement {
    /// The rebuilt array when this call allocated one.
    pub replacement: Option<RuntimeCellHandle>,
    /// Set once the next integer key is occupied and a requested value was not inserted.
    pub overflow: bool,
}

/// Rebuilds an associative array after appending values at the array's stored next index.
///
/// The cursor is the hash's next index, not the maximum live key. A literal
/// `[-5 => "a"]` continues at -4. An array that is still the empty constant keeps
/// next index 0 before PHP 8.3, so the same negative key is followed by 0 there
/// and by -4 from 8.3. Unset does not rewind the cursor. `PHP_INT_MAX` stays the
/// cursor after that key is inserted; the value is stored when the key is free,
/// and only a further insertion is rejected.
pub(in crate::interpreter) fn eval_array_push_assoc_replacement(
    array: RuntimeCellHandle,
    inserted: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<ArrayPushReplacement, EvalStatus> {
    let len = values.array_len(array)?;
    let Some(mut next_key) = values.array_next_index(array)? else {
        return Ok(ArrayPushReplacement { replacement: None, overflow: true });
    };
    let mut result = values.assoc_new(len.saturating_add(inserted.len()))?;
    let mut operands = Vec::new();
    let built = (|| {
        for position in 0..len {
            let key = values.array_iter_key(array, position)?;
            operands.push(key);
            let value = values.array_get(array, key)?;
            operands.push(value);
            result = values.array_set(result, key, value)?;
        }
        let mut maximum_key_inserted = false;
        for value in inserted.iter().copied() {
            if maximum_key_inserted { return Ok(true); }
            let key = values.int(next_key)?;
            operands.push(key);
            result = values.array_set(result, key, value)?;
            maximum_key_inserted = next_key == i64::MAX;
            next_key = next_key.saturating_add(1);
        }
        Ok(false)
    })();
    // Insertion retains the entries, so retire every temporary key and copied value lease.
    // Keep input arguments borrowed and preserve the successfully inserted prefix on overflow.
    let mut cleanup = Ok(());
    for operand in operands {
        let released = release_expr_result(operand, context, values);
        if cleanup.is_ok() { cleanup = released; }
    }
    match (built, cleanup) {
        (Ok(overflow), Ok(())) => Ok(ArrayPushReplacement { replacement: Some(result), overflow }),
        (Err(status), _) | (Ok(_), Err(status)) => {
            let _ = eval_release_value(context, values, result);
            Err(status)
        }
    }
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
    let inserted_args = args[1..].iter().map(EvalCallArg::value).collect::<Vec<_>>();
    with_eval_operands(&inserted_args, context, scope, values, |inserted, context, _, values| {
        let update = eval_array_push_unshift_replacement(name, array, inserted, context, values)?;
        if update.overflow {
            if let Some(replacement) = update.replacement {
                eval_write_direct_ref_target(&target, replacement, context, values, None)?;
            }
            return eval_throw_error(ARRAY_NEXT_ELEMENT_OCCUPIED, context, values);
        }
        let result = eval_array_push_unshift_count_result(array, inserted.len(), values)?;
        if let Some(replacement) = update.replacement {
            eval_write_direct_ref_target(&target, replacement, context, values, None)?;
        }
        Ok(result)
    })
}
