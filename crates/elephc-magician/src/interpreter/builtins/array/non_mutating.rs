//! Purpose:
//! Shared registry hooks for non-mutating array and iterator builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Mutating/by-reference array builtins stay on the source-sensitive legacy
//!   dispatch path until their writable-target handling is migrated.

use super::super::super::*;
use super::super::*;

/// Dispatches direct non-mutating array and iterator calls from declarative specs.
pub(in crate::interpreter) fn eval_builtin_array_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "array_chunk" => eval_builtin_array_chunk(args, context, scope, values),
        "array_column" => eval_builtin_array_column(args, context, scope, values),
        "array_combine" => eval_builtin_array_combine(args, context, scope, values),
        "array_diff" | "array_intersect" => {
            eval_builtin_array_value_set(name, args, context, scope, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            eval_builtin_array_key_set(name, args, context, scope, values)
        }
        "array_fill" => eval_builtin_array_fill(args, context, scope, values),
        "array_fill_keys" => eval_builtin_array_fill_keys(args, context, scope, values),
        "array_filter" => eval_builtin_array_filter(args, context, scope, values),
        "array_map" => eval_builtin_array_map(args, context, scope, values),
        "array_merge" => eval_builtin_array_merge(args, context, scope, values),
        "array_reduce" => eval_builtin_array_reduce(args, context, scope, values),
        "iterator_apply" => eval_builtin_iterator_apply(args, context, scope, values),
        "iterator_count" => eval_builtin_iterator_count(args, context, scope, values),
        "iterator_to_array" => eval_builtin_iterator_to_array(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated non-mutating array and iterator calls from declarative specs.
pub(in crate::interpreter) fn eval_array_non_mutating_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "array_chunk" => {
            let [array, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_chunk_result(*array, *length, values)
        }
        "array_column" => {
            let [array, column_key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_column_result(*array, *column_key, values)
        }
        "array_combine" => {
            let [keys, values_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_combine_result(*keys, *values_array, values)
        }
        "array_diff" | "array_intersect" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_value_set_result(name, *left, *right, values)
        }
        "array_diff_key" | "array_intersect_key" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_key_set_result(name, *left, *right, values)
        }
        "array_fill" => {
            let [start, count, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_result(*start, *count, *value, values)
        }
        "array_fill_keys" => {
            let [keys, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_keys_result(*keys, *value, values)
        }
        "array_filter" => match evaluated_args {
            [array] => eval_array_filter_result(*array, None, None, context, values),
            [array, callback] => {
                eval_array_filter_result(*array, Some(*callback), None, context, values)
            }
            [array, callback, mode] => {
                eval_array_filter_result(*array, Some(*callback), Some(*mode), context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "array_map" => {
            let Some((callback, arrays)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_map_result(*callback, arrays, context, values)
        }
        "array_merge" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_merge_result(*left, *right, values)
        }
        "array_reduce" => match evaluated_args {
            [array, callback] => {
                let initial = values.null()?;
                eval_array_reduce_result(*array, *callback, initial, context, values)
            }
            [array, callback, initial] => {
                eval_array_reduce_result(*array, *callback, *initial, context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "iterator_apply" => match evaluated_args {
            [iterator, callback] => {
                let callback = eval_callable(*callback, context, values)?;
                eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)
            }
            [iterator, callback, args] => {
                let callback = eval_callable(*callback, context, values)?;
                let callback_args = eval_iterator_apply_arg_values(*args, context, values)?;
                eval_iterator_apply_result(*iterator, &callback, callback_args, context, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "iterator_count" => {
            let [iterator] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_iterator_count_result(*iterator, values)
        }
        "iterator_to_array" => match evaluated_args {
            [iterator] => eval_iterator_to_array_result(*iterator, true, values),
            [iterator, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_iterator_to_array_result(*iterator, preserve_keys, values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
