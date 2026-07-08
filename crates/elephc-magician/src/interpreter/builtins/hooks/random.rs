//! Purpose:
//! Focused dispatch helpers for declarative random-number builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::values`.
//!
//! Key details:
//! - `rand` and `mt_rand` accept either no arguments or an inclusive min/max
//!   range, while `random_int` requires an inclusive min/max range.

use super::super::super::{EvalStatus, RuntimeCellHandle, RuntimeValueOps};
use super::super::{eval_rand_result, eval_random_int_result};

/// Dispatches evaluated random-number builtin calls.
pub(super) fn eval_random_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "rand" | "mt_rand" => match evaluated_args {
            [] => eval_rand_result(None, None, values),
            [min, max] => eval_rand_result(Some(*min), Some(*max), values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        "random_int" => {
            let [min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_random_int_result(*min, *max, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
