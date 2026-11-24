//! Purpose:
//! Declarative eval registry entry for `usort`.
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
use crate::runtime_hooks::owner_frames::StagedRuntimeOwners;

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
    callback.with_values(context, values, |callback, context, values| {
        with_collection_result(values, |values, result| {
            with_collection_cells(values, |values, owners| {
                let mut entries = eval_user_sort_entries(array, values, owners)?;
                eval_user_sort_entries_in_place(name, &callback, &mut entries, context, values)?;
                let sorted = if name == "usort" {
                    eval_user_sort_reindex_result(entries, values)?
                } else {
                    eval_user_sort_preserve_key_result(entries, values)?
                };
                result.replace(Some(sorted));
                Ok(())
            })
        })
    })
}

/// Collects source keys and values from one eval array for user sorting.
pub(in crate::interpreter) fn eval_user_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
) -> Result<Vec<EvalUserSortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let source_key = owners.stage(values.array_iter_key(array, position)?);
        let value = owners.stage(values.array_get(array, source_key)?);
        entries.push(EvalUserSortEntry { source_key, value });
    }
    Ok(entries)
}

/// Sorts entries by repeatedly invoking the PHP comparator callback.
pub(in crate::interpreter) fn eval_user_sort_entries_in_place(
    name: &str,
    callback: &EvaluatedCallable,
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
    callback: &EvaluatedCallable,
    left: &EvalUserSortEntry,
    right: &EvalUserSortEntry,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let args = if name == "uksort" {
        vec![left.source_key.borrowed(), right.source_key.borrowed()]
    } else {
        vec![left.value.borrowed(), right.value.borrowed()]
    };
    with_collection_cells(values, |values, owners| {
        let result = owners.stage(eval_evaluated_callable_with_values(callback, args, context, values)?);
        eval_int_value(result, values)
    })
}

/// Builds the reindexed result for `usort()`.
pub(in crate::interpreter) fn eval_user_sort_reindex_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, entries.len())?;
    for (index, entry) in entries.into_iter().enumerate() {
        result.index(index, |_| Ok(entry.value.borrowed()))?;
    }
    Ok(result.finish())
}

/// Builds the key-preserving result for `uksort()` and `uasort()`.
pub(in crate::interpreter) fn eval_user_sort_preserve_key_result(
    entries: Vec<EvalUserSortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::assoc(values, entries.len())?;
    for entry in entries {
        result.entry(|_| Ok(entry.value.borrowed()), |_, _| Ok(entry.source_key.borrowed()))?;
    }
    Ok(result.finish())
}

/// Evaluates direct by-reference user-comparator sort calls and writes back the array.
pub(in crate::interpreter) fn eval_user_sort_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_collection_call(context, values, |context, values, owners, result| {
        let (array, target, callback) = eval_user_sort_direct_args(args, context, scope, values, owners)?;
        result.replace(Some(values.bool_value(true)?));
        let replacement = eval_user_sort_replacement_from_scope(
            name, array, callback, Some(scope), context, values,
        )?;
        write_back_owned_ref_target(&target, replacement, context, values)
    })
}

/// Evaluates and binds direct user-sort arguments while preserving source order.
pub(in crate::interpreter) fn eval_user_sort_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
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
                    arg, context, scope, values, owners,
                )?);
            }
            "callback" => {
                if callback.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let value = eval_owned_expr(arg.value(), context, scope, values)?;
                callback = Some(owners.stage(value).borrowed());
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (array, target) = array.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, target, callback))
}
