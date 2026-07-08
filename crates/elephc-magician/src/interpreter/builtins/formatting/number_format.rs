//! Purpose:
//! Implements PHP `number_format()` evaluation and decimal grouping helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting` re-exports.
//!
//! Key details:
//! - Float coercion is shared with printf formatting, while separator coercion
//!   remains local to `number_format()`.

use super::super::super::*;
use super::super::*;
use super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "number_format",
    area: Formatting,
    params: [
        num,
        decimals = EvalBuiltinDefaultValue::Int(0),
        decimal_separator = EvalBuiltinDefaultValue::String("."),
        thousands_separator = EvalBuiltinDefaultValue::String(","),
    ],
    direct: NumberFormat,
    values: NumberFormat,
}

/// Evaluates PHP `number_format(...)` over one number and optional separators.
pub(in crate::interpreter) fn eval_builtin_number_format(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_number_format_result(value, None, None, None, values)
        }
        [value, decimals] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), None, None, values)
        }
        [value, decimals, decimal_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            eval_number_format_result(value, Some(decimals), Some(decimal_separator), None, values)
        }
        [value, decimals, decimal_separator, thousands_separator] => {
            let value = eval_expr(value, context, scope, values)?;
            let decimals = eval_expr(decimals, context, scope, values)?;
            let decimal_separator = eval_expr(decimal_separator, context, scope, values)?;
            let thousands_separator = eval_expr(thousands_separator, context, scope, values)?;
            eval_number_format_result(
                value,
                Some(decimals),
                Some(decimal_separator),
                Some(thousands_separator),
                values,
            )
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Formats one PHP numeric value with grouped thousands and fixed decimals.
pub(in crate::interpreter) fn eval_number_format_result(
    value: RuntimeCellHandle,
    decimals: Option<RuntimeCellHandle>,
    decimal_separator: Option<RuntimeCellHandle>,
    thousands_separator: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let decimals = match decimals {
        Some(decimals) => eval_int_value(decimals, values)?,
        None => 0,
    };
    let decimal_separator = match decimal_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b".".to_vec(),
    };
    let thousands_separator = match thousands_separator {
        Some(separator) => values.string_bytes(separator)?,
        None => b",".to_vec(),
    };
    let output =
        eval_number_format_bytes(value, decimals, &decimal_separator, &thousands_separator)?;
    values.string_bytes_value(&output)
}

/// Dispatches evaluated `number_format()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_number_format_declared_values_result(
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

/// Produces PHP `number_format()` bytes for finite scalar values.
pub(in crate::interpreter) fn eval_number_format_bytes(
    value: f64,
    decimals: i64,
    decimal_separator: &[u8],
    thousands_separator: &[u8],
) -> Result<Vec<u8>, EvalStatus> {
    if !value.is_finite() {
        return Ok(value.to_string().into_bytes());
    }
    let decimals = decimals.clamp(-308, 308);
    let display_decimals = decimals.max(0) as usize;
    let abs_value = value.abs();
    let scaled = if decimals >= 0 {
        let scale = 10_f64.powi(decimals as i32);
        (abs_value * scale).round()
    } else {
        let scale = 10_f64.powi((-decimals) as i32);
        (abs_value / scale).round() * scale
    };
    if scaled > (u128::MAX as f64) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let scaled = scaled as u128;
    let scale = 10_u128
        .checked_pow(display_decimals as u32)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let integer = if display_decimals == 0 {
        scaled
    } else {
        scaled / scale
    };
    let fraction = if display_decimals == 0 {
        0
    } else {
        scaled % scale
    };

    let mut output = Vec::new();
    if value.is_sign_negative() && scaled != 0 {
        output.push(b'-');
    }
    eval_append_grouped_decimal(&mut output, integer, thousands_separator);
    if display_decimals > 0 {
        output.extend_from_slice(decimal_separator);
        let fraction = format!("{fraction:0display_decimals$}");
        output.extend_from_slice(fraction.as_bytes());
    }
    Ok(output)
}

/// Appends one unsigned decimal integer with optional three-digit grouping.
pub(in crate::interpreter) fn eval_append_grouped_decimal(
    output: &mut Vec<u8>,
    value: u128,
    separator: &[u8],
) {
    let digits = value.to_string();
    if separator.is_empty() {
        output.extend_from_slice(digits.as_bytes());
        return;
    }
    let first_group = match digits.len() % 3 {
        0 => 3,
        len => len,
    };
    output.extend_from_slice(&digits.as_bytes()[..first_group]);
    let mut index = first_group;
    while index < digits.len() {
        output.extend_from_slice(separator);
        output.extend_from_slice(&digits.as_bytes()[index..index + 3]);
        index += 3;
    }
}
