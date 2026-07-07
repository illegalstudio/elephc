//! Purpose:
//! Declarative eval registry entries and dispatch adapters for preg regex builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated regex dispatch.
//!
//! Key details:
//! - Regex parsing, matching, capture assembly, replacement, and split behavior
//!   stay in sibling helper modules.
//! - `preg_match()` and `preg_match_all()` keep source-sensitive direct paths
//!   for `$matches` by-reference writeback.

use super::super::super::*;
use super::*;

mod preg_match;
mod preg_match_all;
mod preg_replace;
mod preg_replace_callback;
mod preg_split;

/// Dispatches direct expression-level calls for declaratively migrated regex builtins.
pub(in crate::interpreter) fn eval_builtin_regex_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "preg_match" => eval_builtin_preg_match(args, context, scope, values),
        "preg_match_all" => eval_builtin_preg_match_all(args, context, scope, values),
        "preg_replace" => eval_builtin_preg_replace(args, context, scope, values),
        "preg_replace_callback" => {
            eval_builtin_preg_replace_callback(args, context, scope, values)
        }
        "preg_split" => eval_builtin_preg_split(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated regex builtins.
pub(in crate::interpreter) fn eval_regex_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "preg_match" => match evaluated_args {
            [pattern, subject] => eval_preg_match_result(*pattern, *subject, values),
            [pattern, subject, _matches] => {
                values.warning(
                    "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
                )?;
                let (matched, matches) =
                    eval_preg_match_capture_result(*pattern, *subject, None, values)?;
                values.release(matches)?;
                Ok(matched)
            }
            [pattern, subject, _matches, flags] => {
                values.warning(
                    "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
                )?;
                let (matched, matches) =
                    eval_preg_match_capture_result(*pattern, *subject, Some(*flags), values)?;
                values.release(matches)?;
                Ok(matched)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "preg_match_all" => match evaluated_args {
            [pattern, subject] => eval_preg_match_all_result(*pattern, *subject, values),
            [pattern, subject, _matches] => {
                values.warning(
                    "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
                )?;
                let (count, matches) =
                    eval_preg_match_all_capture_result(*pattern, *subject, None, values)?;
                values.release(matches)?;
                Ok(count)
            }
            [pattern, subject, _matches, flags] => {
                values.warning(
                    "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
                )?;
                let (count, matches) =
                    eval_preg_match_all_capture_result(*pattern, *subject, Some(*flags), values)?;
                values.release(matches)?;
                Ok(count)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace" => {
            let [pattern, replacement, subject] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_preg_replace_result(*pattern, *replacement, *subject, values)
        }
        "preg_replace_callback" => {
            let [pattern, callback, subject] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_preg_replace_callback_result(*pattern, *callback, *subject, context, values)
        }
        "preg_split" => match evaluated_args {
            [pattern, subject] => eval_preg_split_result(*pattern, *subject, None, None, values),
            [pattern, subject, limit] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), None, values)
            }
            [pattern, subject, limit, flags] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), Some(*flags), values)
            }
            _ => Err(EvalStatus::RuntimeFatal),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
