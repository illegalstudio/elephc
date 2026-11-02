//! Purpose:
//! Declarative eval registry entry for `usort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    contract: "usort",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `usort` array mutator.
pub(in crate::interpreter) fn eval_usort_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    super::array_pop::eval_warn_array_by_value("usort", values)?;
    eval_user_sort_value_result("usort", *array, *callback, context, values)
}

/// Returns the dynamic callable result for by-value user-comparator sort calls.
pub(in crate::interpreter) fn eval_user_sort_value_result(
    name: &str,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let replacement = eval_user_sort_replacement(name, array, callback, context, values)?;
    values.release(replacement)?;
    values.bool_value(true)
}

/// One source array entry used by eval user-comparator sort routines.
pub(in crate::interpreter) struct EvalUserSortEntry {
    source_key: RuntimeCellHandle,
    value: RuntimeCellHandle,
}

/// Builds the sorted replacement array for user-comparator sort builtins.
pub(in crate::interpreter) fn eval_user_sort_replacement(
    name: &str,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_user_sort_replacement_from_scope(name, array, callback, None, context, values)
}

/// Builds the sorted replacement array with optional lexical scope for callback names.
pub(in crate::interpreter) fn eval_user_sort_replacement_from_scope(
    name: &str,
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    // A callable array's receiver read (`[$object, 'method']`) is an owner the dispatch
    // borrows for the whole walk; `with_values` releases it once, after the last callback.
    callback.with_values(context, values, |callback, context, values| {
        let mut entries = eval_user_sort_entries(array, values)?;
        if let Err(status) = eval_user_sort_entries_in_place(name, &callback, &mut entries, context, values)
        {
            for entry in entries {
                let _ = values.release(entry.source_key);
                let _ = values.release(entry.value);
            }
            return Err(status);
        }
        if name == "usort" {
            return eval_user_sort_reindex_result(entries, values);
        }
        eval_user_sort_preserve_key_result(entries, values)
    })
}

/// Collects source keys and values from one eval array for user sorting.
pub(in crate::interpreter) fn eval_user_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalUserSortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        entries.push(EvalUserSortEntry { source_key, value });
    }
    Ok(entries)
}

/// Runs at or below this length are sorted by insertion, as php's hybrid `zend_sort` does.
const EVAL_USER_SORT_INSERTION_RUN: usize = 16;

/// Sorts entries with the PHP comparator callback: stable and O(n log n), like php 8's sort.
///
/// This used to be a bubble sort -- quadratic comparator calls, so a few thousand elements took
/// millions of interpreted callbacks. Runs of up to `EVAL_USER_SORT_INSERTION_RUN` are sorted by
/// binary insertion and then merged bottom-up; ties keep their original order (php's sort is
/// stable since 8.0). A comparator that throws stops the sort with the entries still a
/// permutation of the input, so every owner is still released by the caller.
pub(in crate::interpreter) fn eval_user_sort_entries_in_place(
    name: &str,
    callback: &EvaluatedCallable,
    entries: &mut Vec<EvalUserSortEntry>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = entries.len();
    if len < 2 {
        return Ok(());
    }
    let mut order: Vec<usize> = (0..len).collect();
    let compare = |left: usize, right: usize, context: &mut ElephcEvalContext, values: &mut _| {
        eval_user_sort_compare(name, callback, &entries[left], &entries[right], context, values)
    };
    let outcome = (|| -> Result<(), EvalStatus> {
        // Binary-insertion sort of each short run.
        let mut start = 0;
        while start < len {
            let end = (start + EVAL_USER_SORT_INSERTION_RUN).min(len);
            for index in start + 1..end {
                let item = order[index];
                let (mut low, mut high) = (start, index);
                while low < high {
                    let middle = low + (high - low) / 2;
                    // Insert AFTER equal elements to stay stable.
                    if compare(order[middle], item, context, values)? <= 0 {
                        low = middle + 1;
                    } else {
                        high = middle;
                    }
                }
                order[low..=index].rotate_right(1);
            }
            start = end;
        }
        // Bottom-up merges of sorted runs.
        let mut width = EVAL_USER_SORT_INSERTION_RUN;
        let mut buffer = Vec::with_capacity(len);
        while width < len {
            let mut left = 0;
            while left + width < len {
                let middle = left + width;
                let right = (left + 2 * width).min(len);
                buffer.clear();
                let (mut i, mut j) = (left, middle);
                while i < middle && j < right {
                    // Take from the left run on ties: that is what keeps the merge stable.
                    if compare(order[i], order[j], context, values)? <= 0 {
                        buffer.push(order[i]);
                        i += 1;
                    } else {
                        buffer.push(order[j]);
                        j += 1;
                    }
                }
                buffer.extend_from_slice(&order[i..middle]);
                buffer.extend_from_slice(&order[j..right]);
                order[left..right].copy_from_slice(&buffer);
                left = right;
            }
            width *= 2;
        }
        Ok(())
    })();
    // Apply the permutation (complete, or the partial one a throwing comparator left behind).
    let mut taken: Vec<Option<EvalUserSortEntry>> = entries.drain(..).map(Some).collect();
    for index in order {
        if let Some(entry) = taken[index].take() {
            entries.push(entry);
        }
    }
    outcome
}

/// Invokes one user-sort comparator and returns its integer ordering result.
pub(in crate::interpreter) fn eval_user_sort_compare(
    name: &str,
    callback: &EvaluatedCallable,
    left: &EvalUserSortEntry,
    right: &EvalUserSortEntry,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let args = if name == "uksort" {
        vec![left.source_key, right.source_key]
    } else {
        vec![left.value, right.value]
    };
    let result = eval_evaluated_callable_with_values(callback, args, context, values)?;
    let ordering = eval_int_value(result, values);
    // The comparator's return value is this call's owner, whatever it converts to.
    values.release(result)?;
    ordering
}

/// Builds the reindexed result for `usort()`.
pub(in crate::interpreter) fn eval_user_sort_reindex_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(entries.len())?;
    for (index, entry) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, entry.value)?;
        // `array_set` retained the value; the entry's own key and value reads are released.
        values.release(key)?;
        values.release(entry.source_key)?;
        values.release(entry.value)?;
    }
    Ok(result)
}

/// Builds the key-preserving result for `uksort()` and `uasort()`.
pub(in crate::interpreter) fn eval_user_sort_preserve_key_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(entries.len())?;
    for entry in entries {
        result = values.array_set(result, entry.source_key, entry.value)?;
        values.release(entry.source_key)?;
        values.release(entry.value)?;
    }
    Ok(result)
}

/// Evaluates direct by-reference user-comparator sort calls and writes back the array.
pub(in crate::interpreter) fn eval_user_sort_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, target, callback) = eval_user_sort_direct_args(args, context, scope, values)?;

    let replacement = eval_user_sort_replacement_from_scope(
        name,
        array,
        callback,
        Some(scope),
        context,
        values,
    );
    // An evaluated comparator expression (`fn ($a, $b) => ...` written in the call) is this
    // call's owner; a borrowed variable read is not.
    if !callback.is_borrowed() {
        values.release(callback)?;
    }
    let replacement = replacement?;
    let result = values.bool_value(true)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}

/// Evaluates and binds direct user-sort arguments while preserving source order.
pub(in crate::interpreter) fn eval_user_sort_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    let mut array = None;
    let mut callback = None;
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
                1 => "callback",
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
                    arg, context, scope, values,
                )?);
            }
            "callback" => {
                if callback.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                callback = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (array, target) = array.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, target, callback))
}
