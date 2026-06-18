//! Purpose:
//! Dispatches already evaluated numeric formatting and printf-family builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated numeric formatting and printf-family builtins.
pub(in crate::interpreter) fn eval_formatting_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "ceil" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.ceil(*value)?
        }
        "floor" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.floor(*value)?
        }
        "pi" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.float(std::f64::consts::PI)?
        }
        "pow" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.pow(*left, *right)?
        }
        "round" => match evaluated_args {
            [value] => values.round(*value, None)?,
            [value, precision] => values.round(*value, Some(*precision))?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "sscanf" => {
            let [input, format, ..] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sscanf_result(*input, *format, values)?
        }
        "sprintf" | "printf" => eval_sprintf_like_result(name, evaluated_args, values)?,
        "number_format" => match evaluated_args {
            [value] => eval_number_format_result(*value, None, None, None, values)?,
            [value, decimals] => {
                eval_number_format_result(*value, Some(*decimals), None, None, values)?
            }
            [value, decimals, decimal_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                None,
                values,
            )?,
            [value, decimals, decimal_separator, thousands_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                Some(*thousands_separator),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "vsprintf" | "vprintf" => eval_vsprintf_like_result(name, evaluated_args, values)?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}
