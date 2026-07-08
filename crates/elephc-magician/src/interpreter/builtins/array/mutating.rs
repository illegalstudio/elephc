//! Purpose:
//! Values-only registry hook for mutating/by-reference array builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive writable-target path; this hook
//!   preserves callable by-value warning behavior.

use super::super::super::*;
use super::super::*;

/// Dispatches by-value callable calls for mutating array builtins.
pub(in crate::interpreter) fn eval_array_mutating_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "array_walk" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(
                "array_walk(): Argument #1 ($array) must be passed by reference, value given",
            )?;
            eval_array_walk_result(*array, *callback, context, values)
        }
        "array_pop" | "array_shift" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            warn_array_by_value(name, values)?;
            eval_array_pop_shift_value_result(name, *array, values)
        }
        "array_push" | "array_unshift" => {
            let Some((array, inserted)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            warn_array_by_value(name, values)?;
            eval_array_push_unshift_count_result(*array, inserted.len(), values)
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
            Ok(result)
        }
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            warn_array_by_value(name, values)?;
            eval_array_sort_value_result(*array, values)
        }
        "uasort" | "uksort" | "usort" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            warn_array_by_value(name, values)?;
            eval_user_sort_value_result(name, *array, *callback, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Emits the standard by-value warning for array mutators.
fn warn_array_by_value(name: &str, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "{name}(): Argument #1 ($array) must be passed by reference, value given"
    ))
}
