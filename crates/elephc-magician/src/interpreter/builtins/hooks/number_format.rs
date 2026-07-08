//! Purpose:
//! Focused evaluated-argument dispatch helper for `number_format`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::values`.
//!
//! Key details:
//! - Keeping the arity match here prevents the generic values hook table from
//!   growing past the ordinary file-size limit.

use super::super::super::{EvalStatus, RuntimeCellHandle, RuntimeValueOps};
use super::super::eval_number_format_result;

/// Dispatches evaluated `number_format` arguments.
pub(super) fn eval_number_format_values(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_number_format_result(*value, None, None, None, values),
        [value, decimals] => {
            eval_number_format_result(*value, Some(*decimals), None, None, values)
        }
        [value, decimals, decimal_separator] => eval_number_format_result(
            *value,
            Some(*decimals),
            Some(*decimal_separator),
            None,
            values,
        ),
        [value, decimals, decimal_separator, thousands_separator] => eval_number_format_result(
            *value,
            Some(*decimals),
            Some(*decimal_separator),
            Some(*thousands_separator),
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
