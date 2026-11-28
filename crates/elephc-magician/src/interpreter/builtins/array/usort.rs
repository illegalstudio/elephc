//! Purpose:
//! Declarative eval registry entry for `usort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

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
pub(in crate::interpreter) type EvalUserSortEntry = super::sort::EvalArraySortEntry<()>;

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
        let entries = super::sort::owners::collect(array, context, values, |_, _, _| Ok(()))?;
        super::sort::owners::with_entries(entries, context, values, |entries, context, values| {
            eval_user_sort_entries_in_place(name, &callback, entries, context, values)?;
            super::sort::owners::build_result(entries, name != "usort", context, values)
        })
    })
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
        vec![left.source_key, right.source_key]
    } else {
        vec![left.value, right.value]
    };
    let result = RuntimeOwnerSlot::new(eval_evaluated_callable_with_values(callback, args, context, values)?);
    with_runtime_owners(&result, || {
        let comparison = (|| {
            if context.semantic_profile().contains(elephc_semantics::StrictFeature::Comparisons)
                && values.type_tag(result.borrowed())? != EVAL_TAG_INT
            {
                return eval_throw_type_error(elephc_semantics::comparisons::COMPARATOR_MESSAGE, context, values);
            }
            eval_int_value(result.borrowed(), values)
        })();
        let released = release_expr_result(result.take(), context, values);
        comparison.and_then(|comparison| released.map(|()| comparison))
    })
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

    let callback = RuntimeOwnerSlot::new(callback);
    with_runtime_owners(&callback, || {
        let result = super::super::ref_targets::with_direct_ref_target(target, context, values,
            |target, context, values| {
                let replacement = eval_user_sort_replacement_from_scope(
                    name, array, callback.borrowed(), Some(scope), context, values)?;
                let result = values.bool_value(true);
                match result {
                    Ok(result) => super::super::ref_targets::finish_owned_direct_ref_replacement(
                        target, replacement, result, context, values),
                    Err(status) => finish_eval_argument_values(Err(status), [replacement], context, values),
                }
            });
        finish_eval_argument_values(result, [callback.take()], context, values)
    })
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
