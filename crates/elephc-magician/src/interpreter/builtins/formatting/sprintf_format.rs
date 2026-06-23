//! Purpose:
//! Parses and applies printf-style format specifiers for eval printf-family calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting::printf`.
//!
//! Key details:
//! - Formatting uses PHP runtime coercions through `RuntimeValueOps` before width,
//!   precision, sign, and alternate-form handling are applied.

use super::super::super::*;
use super::super::*;
use super::*;

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
