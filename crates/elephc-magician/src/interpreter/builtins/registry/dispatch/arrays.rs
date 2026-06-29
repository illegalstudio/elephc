//! Purpose:
//! Dispatches already evaluated array and iterator builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;
use super::super::*;

/// Attempts to dispatch evaluated array and iterator builtins.
pub(in crate::interpreter) fn eval_arrays_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "array_combine" => {
            let [keys, values_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_combine_result(*keys, *values_array, values)?
        }
        "array_column" => {
            let [array, column_key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_column_result(*array, *column_key, values)?
        }
        "array_chunk" => {
            let [array, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_chunk_result(*array, *length, values)?
        }
        "array_fill" => {
            let [start, count, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_result(*start, *count, *value, values)?
        }
        "array_fill_keys" => {
            let [keys, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_keys_result(*keys, *value, values)?
        }
        "array_filter" => match evaluated_args {
            [array] => eval_array_filter_result(*array, None, None, context, values)?,
            [array, callback] => {
                eval_array_filter_result(*array, Some(*callback), None, context, values)?
            }
            [array, callback, mode] => {
                eval_array_filter_result(*array, Some(*callback), Some(*mode), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_map" => {
            let Some((callback, arrays)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_map_result(*callback, arrays, context, values)?
        }
        "array_reduce" => match evaluated_args {
            [array, callback] => {
                let initial = values.null()?;
                eval_array_reduce_result(*array, *callback, initial, context, values)?
            }
            [array, callback, initial] => {
                eval_array_reduce_result(*array, *callback, *initial, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_walk" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(
                "array_walk(): Argument #1 ($array) must be passed by reference, value given",
            )?;
            eval_array_walk_result(*array, *callback, context, values)?
        }
        "array_pop" | "array_shift" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_pop_shift_value_result(name, *array, values)?
        }
        "array_push" | "array_unshift" => {
            let Some((array, inserted)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_push_unshift_count_result(*array, inserted.len(), values)?
        }
        "array_splice" => {
            let result = match evaluated_args {
                [array, offset] => eval_array_splice_value_result(*array, *offset, None, values)?,
                [array, offset, length] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                [array, offset, length, _replacement] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            values.warning(
                "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            )?;
            result
        }
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_sort_value_result(*array, values)?
        }
        "uasort" | "uksort" | "usort" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_user_sort_value_result(name, *array, *callback, context, values)?
        }
        "array_flip" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_flip_result(*array, values)?
        }
        "array_pad" => {
            let [array, length, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_pad_result(*array, *length, *value, values)?
        }
        "array_product" | "array_sum" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_aggregate_result(name, *array, values)?
        }
        "array_keys" | "array_values" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_projection_result(name, *array, values)?
        }
        "array_key_exists" => {
            let [key, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, *array)?
        }
        "array_diff" | "array_intersect" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_value_set_result(name, *left, *right, values)?
        }
        "array_diff_key" | "array_intersect_key" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_key_set_result(name, *left, *right, values)?
        }
        "array_merge" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_merge_result(*left, *right, values)?
        }
        "array_rand" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_rand_result(*array, values)?
        }
        "array_reverse" => match evaluated_args {
            [array] => eval_array_reverse_result(*array, false, values)?,
            [array, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_array_reverse_result(*array, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_search" | "in_array" => {
            let [needle, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_search_result(name, *needle, *array, values)?
        }
        "array_slice" => match evaluated_args {
            [array, offset] => eval_array_slice_result(*array, *offset, None, values)?,
            [array, offset, length] => {
                eval_array_slice_result(*array, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_unique" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_unique_result(*array, values)?
        }
        "range" => {
            let [start, end] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_range_result(*start, *end, values)?
        }
        "count" => match evaluated_args {
            [value] => eval_count_result(*value, None, context, values)?,
            [value, mode] => eval_count_result(*value, Some(*mode), context, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "iterator_apply" => match evaluated_args {
            [iterator, callback] => {
                let callback = eval_callable(*callback, context, values)?;
                eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)?
            }
            [iterator, callback, args] => {
                let callback = eval_callable(*callback, context, values)?;
                let callback_args = eval_iterator_apply_arg_values(*args, values)?;
                eval_iterator_apply_result(*iterator, &callback, callback_args, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "iterator_count" => {
            let [iterator] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_iterator_count_result(*iterator, values)?
        }
        "iterator_to_array" => match evaluated_args {
            [iterator] => eval_iterator_to_array_result(*iterator, true, values)?,
            [iterator, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_iterator_to_array_result(*iterator, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
