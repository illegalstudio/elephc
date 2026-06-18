//! Purpose:
//! Implements result construction for PHP `sprintf`, `printf`, `vsprintf`, and
//! `vprintf` eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting::dispatch`.
//!
//! Key details:
//! - The formatted byte stream is shared by string-returning and echoing variants;
//!   echoing variants return the emitted byte count.

use super::super::super::*;
use super::*;

/// Formats `sprintf()` arguments and returns the resulting PHP string.
pub(in crate::interpreter) fn eval_sprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    values.string_bytes_value(&output)
}

/// Formats `printf()` arguments, echoes the result, and returns its byte count.
pub(in crate::interpreter) fn eval_printf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((format, format_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let output = eval_sprintf_bytes(&format, format_args, values)?;
    let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.int(len)
}

/// Formats `vsprintf()` array arguments and returns the resulting PHP string.
pub(in crate::interpreter) fn eval_vsprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let format_args = eval_sprintf_argument_array_values(*array, values)?;
    let output = eval_sprintf_bytes(&format, &format_args, values)?;
    values.string_bytes_value(&output)
}

/// Formats `vprintf()` array arguments, echoes the result, and returns its byte count.
pub(in crate::interpreter) fn eval_vprintf_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [format, array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let format = values.string_bytes(*format)?;
    let format_args = eval_sprintf_argument_array_values(*array, values)?;
    let output = eval_sprintf_bytes(&format, &format_args, values)?;
    let len = i64::try_from(output.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    let output = values.string_bytes_value(&output)?;
    values.echo(output)?;
    values.int(len)
}

/// Reads `vsprintf()` values in PHP array iteration order while ignoring keys.
pub(in crate::interpreter) fn eval_sprintf_argument_array_values(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !values.is_array_like(array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    let mut args = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        args.push(values.array_get(array, key)?);
    }
    Ok(args)
}

/// Formats one printf-style byte string through eval runtime value coercions.
pub(in crate::interpreter) fn eval_sprintf_bytes(
    format: &[u8],
    args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let mut output = Vec::new();
    let mut index = 0;
    let mut arg_index = 0;
    while index < format.len() {
        if format[index] != b'%' {
            output.push(format[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= format.len() {
            break;
        }
        if format[index] == b'%' {
            output.push(b'%');
            index += 1;
            continue;
        }

        let (spec, next_index) = eval_parse_sprintf_spec(format, index)?;
        index = next_index;
        let Some(arg) = args.get(arg_index).copied() else {
            return Err(EvalStatus::RuntimeFatal);
        };
        arg_index += 1;
        let bytes = eval_format_sprintf_arg(spec, arg, values)?;
        output.extend_from_slice(&bytes);
    }
    Ok(output)
}
