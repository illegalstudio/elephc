//! Purpose:
//! Numeric formatting, sprintf-family, and math wrapper builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Helpers stay scoped to the eval interpreter and preserve PHP-visible runtime
//!   behavior through `RuntimeValueOps`.

use super::super::*;
use super::*;

/// Evaluates PHP's `ceil(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.ceil(value)
}

/// Evaluates PHP's `floor(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.floor(value)
}

/// Evaluates PHP's zero-argument `pi()` builtin.
pub(in crate::interpreter) fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.float(std::f64::consts::PI)
}

/// Evaluates PHP's `pow(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_pow(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    values.pow(left, right)
}

/// Evaluates PHP's `round(...)` over one value and an optional precision expression.
pub(in crate::interpreter) fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            values.round(value, None)
        }
        [value, precision] => {
            let value = eval_expr(value, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            values.round(value, Some(precision))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
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

/// Evaluates direct positional `sprintf()` or `printf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_sprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sprintf_like_result(name, &evaluated_args, values)
}

/// Evaluates direct positional `vsprintf()` or `vprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_vsprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vsprintf_like_result(name, &evaluated_args, values)
}

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

/// Dispatches already evaluated `sprintf()` or `printf()` arguments.
pub(in crate::interpreter) fn eval_sprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "sprintf" => eval_sprintf_result(evaluated_args, values),
        "printf" => eval_printf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Dispatches already evaluated `vsprintf()` or `vprintf()` arguments.
pub(in crate::interpreter) fn eval_vsprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "vsprintf" => eval_vsprintf_result(evaluated_args, values),
        "vprintf" => eval_vprintf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
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

/// Parses flags, width, precision, and terminal type for one format specifier.
pub(in crate::interpreter) fn eval_parse_sprintf_spec(
    format: &[u8],
    mut index: usize,
) -> Result<(EvalSprintfSpec, usize), EvalStatus> {
    let mut spec = EvalSprintfSpec {
        left_align: false,
        force_sign: false,
        space_sign: false,
        zero_pad: false,
        alternate: false,
        width: None,
        precision: None,
        specifier: 0,
    };
    while index < format.len() {
        match format[index] {
            b'-' => spec.left_align = true,
            b'+' => spec.force_sign = true,
            b' ' => spec.space_sign = true,
            b'0' => spec.zero_pad = true,
            b'#' => spec.alternate = true,
            _ => break,
        }
        index += 1;
    }
    let (width, next_index) = eval_parse_sprintf_number(format, index)?;
    spec.width = width;
    index = next_index;
    if index < format.len() && format[index] == b'.' {
        let (precision, next_index) = eval_parse_sprintf_number(format, index + 1)?;
        spec.precision = Some(precision.unwrap_or(0));
        index = next_index;
    }
    if index >= format.len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    spec.specifier = format[index];
    Ok((spec, index + 1))
}

/// Parses an unsigned decimal number from a format specifier component.
pub(in crate::interpreter) fn eval_parse_sprintf_number(
    format: &[u8],
    mut index: usize,
) -> Result<(Option<usize>, usize), EvalStatus> {
    let start = index;
    let mut value = 0usize;
    while index < format.len() && format[index].is_ascii_digit() {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(usize::from(format[index] - b'0')))
            .ok_or(EvalStatus::RuntimeFatal)?;
        index += 1;
    }
    if index == start {
        Ok((None, index))
    } else {
        Ok((Some(value), index))
    }
}

/// Formats one runtime value according to a parsed eval sprintf specifier.
pub(in crate::interpreter) fn eval_format_sprintf_arg(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    match spec.specifier {
        b's' => eval_format_sprintf_string(spec, value, values),
        b'f' | b'e' | b'g' => eval_format_sprintf_float(spec, value, values),
        b'c' => eval_format_sprintf_char(spec, value, values),
        _ => eval_format_sprintf_int(spec, value, values),
    }
}

/// Formats a `%s` operand after PHP string coercion.
pub(in crate::interpreter) fn eval_format_sprintf_string(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    if let Some(precision) = spec.precision {
        bytes.truncate(precision);
    }
    Ok(eval_sprintf_apply_width(bytes, spec, false))
}

/// Formats an integer-like operand for decimal, unsigned, hex, and octal specifiers.
pub(in crate::interpreter) fn eval_format_sprintf_int(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_int_value(value, values)?;
    let mut output = Vec::new();
    match spec.specifier {
        b'u' => {
            let digits = eval_sprintf_precision_pad((value as u64).to_string().into_bytes(), spec);
            output.extend_from_slice(&digits);
        }
        b'x' | b'X' => {
            let unsigned = value as u64;
            if spec.alternate && unsigned != 0 {
                output.extend_from_slice(if spec.specifier == b'X' { b"0X" } else { b"0x" });
            }
            let digits = if spec.specifier == b'X' {
                format!("{unsigned:X}").into_bytes()
            } else {
                format!("{unsigned:x}").into_bytes()
            };
            output.extend_from_slice(&eval_sprintf_precision_pad(digits, spec));
        }
        b'o' => {
            let unsigned = value as u64;
            let mut digits = eval_sprintf_precision_pad(format!("{unsigned:o}").into_bytes(), spec);
            if spec.alternate && !digits.starts_with(b"0") {
                output.push(b'0');
            }
            output.append(&mut digits);
        }
        _ => {
            let value = value as i128;
            let magnitude = if value < 0 {
                (-value) as u128
            } else {
                value as u128
            };
            if value < 0 {
                output.push(b'-');
            } else if spec.force_sign {
                output.push(b'+');
            } else if spec.space_sign {
                output.push(b' ');
            }
            let digits = eval_sprintf_precision_pad(magnitude.to_string().into_bytes(), spec);
            output.extend_from_slice(&digits);
        }
    }
    Ok(eval_sprintf_apply_width(output, spec, true))
}

/// Formats a `%c` operand as the low byte of its integer value.
pub(in crate::interpreter) fn eval_format_sprintf_char(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_int_value(value, values)?;
    Ok(eval_sprintf_apply_width(vec![value as u8], spec, false))
}

/// Formats a floating-point operand for the eval printf-family subset.
pub(in crate::interpreter) fn eval_format_sprintf_float(
    spec: EvalSprintfSpec,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    let value = eval_float_value(value, values)?;
    let precision = spec.precision.unwrap_or(6);
    let mut output = if value.is_nan() {
        b"NAN".to_vec()
    } else if value.is_infinite() {
        b"INF".to_vec()
    } else {
        match spec.specifier {
            b'e' => format!("{value:.precision$e}").into_bytes(),
            b'g' => format!("{value:.precision$}").into_bytes(),
            _ => format!("{value:.precision$}").into_bytes(),
        }
    };
    if value.is_sign_negative() && !output.starts_with(b"-") {
        output.insert(0, b'-');
    } else if value.is_sign_positive() && value.is_finite() {
        if spec.force_sign {
            output.insert(0, b'+');
        } else if spec.space_sign {
            output.insert(0, b' ');
        }
    }
    Ok(eval_sprintf_apply_width(output, spec, true))
}

/// Applies integer precision by left-padding digits with zeros.
pub(in crate::interpreter) fn eval_sprintf_precision_pad(
    mut digits: Vec<u8>,
    spec: EvalSprintfSpec,
) -> Vec<u8> {
    if matches!(spec.precision, Some(0)) && digits == b"0" {
        digits.clear();
        return digits;
    }
    let Some(precision) = spec.precision else {
        return digits;
    };
    if digits.len() >= precision {
        return digits;
    }
    let mut output = vec![b'0'; precision - digits.len()];
    output.append(&mut digits);
    output
}

/// Applies field width and alignment to one formatted eval sprintf replacement.
pub(in crate::interpreter) fn eval_sprintf_apply_width(
    mut bytes: Vec<u8>,
    spec: EvalSprintfSpec,
    numeric: bool,
) -> Vec<u8> {
    let Some(width) = spec.width else {
        return bytes;
    };
    if bytes.len() >= width {
        return bytes;
    }
    let pad_len = width - bytes.len();
    if spec.left_align {
        bytes.extend(std::iter::repeat_n(b' ', pad_len));
        return bytes;
    }
    if numeric && spec.zero_pad && spec.precision.is_none() {
        let prefix_len = eval_sprintf_zero_pad_prefix_len(&bytes);
        let mut output = Vec::with_capacity(width);
        output.extend_from_slice(&bytes[..prefix_len]);
        output.extend(std::iter::repeat_n(b'0', pad_len));
        output.extend_from_slice(&bytes[prefix_len..]);
        return output;
    }
    let mut output = Vec::with_capacity(width);
    output.extend(std::iter::repeat_n(b' ', pad_len));
    output.append(&mut bytes);
    output
}

/// Returns the sign and alternate-prefix length that should precede zero padding.
pub(in crate::interpreter) fn eval_sprintf_zero_pad_prefix_len(bytes: &[u8]) -> usize {
    let mut prefix_len = usize::from(matches!(bytes.first(), Some(b'+' | b'-' | b' ')));
    if bytes.len() >= prefix_len + 2
        && bytes[prefix_len] == b'0'
        && matches!(bytes[prefix_len + 1], b'x' | b'X')
    {
        prefix_len += 2;
    }
    prefix_len
}

/// Converts one eval value to PHP float and returns the scalar payload.
pub(in crate::interpreter) fn eval_float_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<f64, EvalStatus> {
    let value = values.cast_float(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<f64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
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
