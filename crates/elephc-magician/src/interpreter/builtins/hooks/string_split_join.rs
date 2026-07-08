//! Purpose:
//! Focused dispatch helpers for declarative `explode` and `implode` hooks.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::values`.
//!
//! Key details:
//! - The eval implementation still requires the currently supported two-argument
//!   runtime form even though signature metadata exposes PHP-compatible defaults.

use super::super::super::{EvalStatus, RuntimeCellHandle, RuntimeValueOps};
use super::super::{eval_explode_result, eval_implode_result};

/// Dispatches evaluated `explode` and `implode` calls.
pub(super) fn eval_string_split_join_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "explode" => {
            let [separator, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_explode_result(*separator, *string, values)
        }
        "implode" => {
            let [separator, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_implode_result(*separator, *array, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
