//! Purpose:
//! Implements eval support for PHP `sscanf()` and its small scanning subset.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting` re-exports.
//!
//! Key details:
//! - Only the currently supported `%d`, `%f`, `%s`, and `%%` subset is parsed;
//!   extra output variables are evaluated for side effects and ignored.

use super::super::super::*;

/// Evaluates direct positional `sscanf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_sscanf(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let input = eval_expr(&args[0], context, scope, values)?;
    let format = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        eval_expr(arg, context, scope, values)?;
    }
    eval_sscanf_result(input, format, values)
}

/// Parses one string through the eval `sscanf()` subset and returns an indexed array.
pub(in crate::interpreter) fn eval_sscanf_result(
    input: RuntimeCellHandle,
    format: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let input = values.string_bytes(input)?;
    let format = values.string_bytes(format)?;
    let matches = eval_sscanf_matches(&input, &format);
    let mut result = values.array_new(matches.len())?;
    for (index, matched) in matches.iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = values.string_bytes_value(matched)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Extracts `%d`, `%f`, `%s`, and `%%` matches with the same subset as native `sscanf()`.
pub(in crate::interpreter) fn eval_sscanf_matches(input: &[u8], format: &[u8]) -> Vec<Vec<u8>> {
    let mut matches = Vec::new();
    let mut input_index = 0;
    let mut format_index = 0;

    while format_index < format.len() {
        if format[format_index] != b'%' {
            if input_index >= input.len() || input[input_index] != format[format_index] {
                break;
            }
            input_index += 1;
            format_index += 1;
            continue;
        }

        format_index += 1;
        if format_index >= format.len() {
            break;
        }

        match format[format_index] {
            b'%' => {
                if input_index >= input.len() || input[input_index] != b'%' {
                    break;
                }
                input_index += 1;
            }
            b'd' => matches.push(eval_sscanf_scan_int(input, &mut input_index)),
            b'f' => matches.push(eval_sscanf_scan_float(input, &mut input_index)),
            b's' => matches.push(eval_sscanf_scan_word(input, &mut input_index)),
            _ => {}
        }
        format_index += 1;
    }

    matches
}

/// Scans the native `sscanf()` `%d` subset as a matched byte slice.
pub(in crate::interpreter) fn eval_sscanf_scan_int(
    input: &[u8],
    input_index: &mut usize,
) -> Vec<u8> {
    let start = *input_index;
    if input.get(*input_index) == Some(&b'-') {
        *input_index += 1;
    }
    while input
        .get(*input_index)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        *input_index += 1;
    }
    input[start..*input_index].to_vec()
}

/// Scans the native `sscanf()` `%f` subset as a matched byte slice.
pub(in crate::interpreter) fn eval_sscanf_scan_float(
    input: &[u8],
    input_index: &mut usize,
) -> Vec<u8> {
    let start = *input_index;
    if input
        .get(*input_index)
        .is_some_and(|byte| matches!(byte, b'+' | b'-'))
    {
        *input_index += 1;
    }
    while input
        .get(*input_index)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        *input_index += 1;
    }
    if input.get(*input_index) == Some(&b'.') {
        *input_index += 1;
        while input
            .get(*input_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            *input_index += 1;
        }
    }
    if input
        .get(*input_index)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        *input_index += 1;
        if input
            .get(*input_index)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            *input_index += 1;
        }
        while input
            .get(*input_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            *input_index += 1;
        }
    }
    input[start..*input_index].to_vec()
}

/// Scans the native `sscanf()` `%s` subset as a non-space byte word.
pub(in crate::interpreter) fn eval_sscanf_scan_word(
    input: &[u8],
    input_index: &mut usize,
) -> Vec<u8> {
    let start = *input_index;
    while input
        .get(*input_index)
        .is_some_and(|byte| !matches!(byte, b' ' | b'\t' | b'\n'))
    {
        *input_index += 1;
    }
    input[start..*input_index].to_vec()
}
