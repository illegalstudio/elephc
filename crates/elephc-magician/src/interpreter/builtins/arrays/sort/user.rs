//! Purpose:
//! Implements user-comparator array sorting for `usort`, `uasort`, and `uksort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::sort` re-exports.
//!
//! Key details:
//! - Comparator callbacks are invoked through the eval context and their integer
//!   return values drive a stable entry ordering.

use super::super::super::super::*;
use super::super::super::*;

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
    let callback = eval_callable_name(callback, values)?;
    let mut entries = eval_user_sort_entries(array, values)?;
    eval_user_sort_entries_in_place(name, &callback, &mut entries, context, values)?;
    if name == "usort" {
        return eval_user_sort_reindex_result(entries, values);
    }
    eval_user_sort_preserve_key_result(entries, values)
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

/// Sorts entries by repeatedly invoking the PHP comparator callback.
pub(in crate::interpreter) fn eval_user_sort_entries_in_place(
    name: &str,
    callback: &str,
    entries: &mut [EvalUserSortEntry],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for pass in 0..entries.len() {
        let upper = entries.len().saturating_sub(pass + 1);
        for index in 0..upper {
            let comparison = eval_user_sort_compare(
                name,
                callback,
                &entries[index],
                &entries[index + 1],
                context,
                values,
            )?;
            if comparison > 0 {
                entries.swap(index, index + 1);
            }
        }
    }
    Ok(())
}

/// Invokes one user-sort comparator and returns its integer ordering result.
pub(in crate::interpreter) fn eval_user_sort_compare(
    name: &str,
    callback: &str,
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
    let result = eval_callable_with_values(callback, args, context, values)?;
    eval_int_value(result, values)
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
    }
    Ok(result)
}
