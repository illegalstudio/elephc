//! Purpose:
//! Dispatches already evaluated preg regex builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated preg regex builtins.
pub(in crate::interpreter) fn eval_regex_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "preg_match" => match evaluated_args {
            [pattern, subject] => eval_preg_match_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_match_all" => match evaluated_args {
            [pattern, subject] => eval_preg_match_all_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace" => match evaluated_args {
            [pattern, replacement, subject] => {
                eval_preg_replace_result(*pattern, *replacement, *subject, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace_callback" => match evaluated_args {
            [pattern, callback, subject] => {
                eval_preg_replace_callback_result(*pattern, *callback, *subject, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_split" => match evaluated_args {
            [pattern, subject] => eval_preg_split_result(*pattern, *subject, None, None, values)?,
            [pattern, subject, limit] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), None, values)?
            }
            [pattern, subject, limit, flags] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
