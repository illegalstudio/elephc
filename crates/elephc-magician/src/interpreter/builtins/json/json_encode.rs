//! Purpose:
//! Eval registry entry and dispatch wrappers for `json_encode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns result/error handling, scalar formatting, direct wrappers,
//!   and the by-value dispatch shape.
//! - Container traversal lives in a stack-safe sibling module.
//! - Shared `JSON_THROW_ON_ERROR` exception construction is reused from
//!   `json_decode` instead of a separate area-level helper module.

use super::super::super::*;

mod errors;
mod objects;
mod ownership;
mod traversal;
use errors::{EvalJsonEncodeFailure, EvalJsonEncodeResult, eval_json_encode_record_error};
use traversal::eval_json_encode_append;
eval_builtin! {
    contract: "json_encode",
    area: Json,
    direct: JsonEncode,
    values: JsonEncode,
}

/// Evaluates PHP `json_encode()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_json_encode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_json_encode_result(value, None, None, context, values)
        }
        [value, flags] => {
            let value = eval_expr(value, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_encode_result(value, Some(flags), None, context, values)
        }
        [value, flags, depth] => {
            let value = eval_expr(value, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_encode_result(value, Some(flags), Some(depth), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches by-value `json_encode()` calls after argument binding.
pub(in crate::interpreter) fn eval_json_encode_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_json_encode_result(*value, None, None, context, values),
        [value, flags] => eval_json_encode_result(*value, Some(*flags), None, context, values),
        [value, flags, depth] => eval_json_encode_result(*value, Some(*flags), Some(*depth), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Encodes one runtime cell as a JSON string for eval's supported flag subset.
fn eval_json_encode_result(
    value: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    depth: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    let supported_flags = EVAL_JSON_HEX_TAG
        | EVAL_JSON_HEX_AMP
        | EVAL_JSON_HEX_APOS
        | EVAL_JSON_HEX_QUOT
        | EVAL_JSON_UNESCAPED_SLASHES
        | EVAL_JSON_UNESCAPED_UNICODE
        | EVAL_JSON_UNESCAPED_LINE_TERMINATORS
        | EVAL_JSON_FORCE_OBJECT
        | EVAL_JSON_PRETTY_PRINT
        | EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR
        | EVAL_JSON_PRESERVE_ZERO_FRACTION
        | EVAL_JSON_INVALID_UTF8_IGNORE
        | EVAL_JSON_INVALID_UTF8_SUBSTITUTE
        | EVAL_JSON_THROW_ON_ERROR;
    let supported_flags = supported_flags | EVAL_JSON_NUMERIC_CHECK;
    if flags & !supported_flags != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut output = Vec::new();
    let mut error = None;
    match eval_json_encode_append(
        value,
        context,
        values,
        flags,
        depth as usize,
        0,
        &mut error,
        &mut output,
    ) {
        Ok(()) => {}
        Err(EvalJsonEncodeFailure::Json(failure)) => error = Some(failure),
        Err(EvalJsonEncodeFailure::Runtime(status)) => {
            // PHP publishes a pending JSON error after a callback throws, without
            // exposing that error to the callback while encoding is still active.
            if flags & EVAL_JSON_THROW_ON_ERROR == 0
                || flags & EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR != 0
            {
                if let Some(error) = error {
                    context.set_json_error(error.code, error.message);
                }
            }
            return Err(status);
        }
    }
    if let Some(error) = error {
        context.set_json_error(error.code, error.message);
        if flags & EVAL_JSON_PARTIAL_OUTPUT_ON_ERROR == 0 {
            if flags & EVAL_JSON_THROW_ON_ERROR != 0 {
                return super::json_decode::eval_throw_json_exception(error.code, error.message, context, values);
            }
            return values.bool_value(false);
        }
    } else {
        context.clear_json_error();
    }
    values.string_bytes_value(&output)
}

#[derive(Clone, Copy)]
struct EvalJsonEncodeError {
    code: i64,
    message: &'static str,
}

/// Marks whether a JSON string is being encoded as a value or as an object key.
#[derive(Clone, Copy)]
enum EvalJsonStringPosition {
    Value,
    Key,
}

/// Appends one JSON float while preserving a `.0` suffix when requested.
fn eval_json_encode_append_float(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    let float = f64::from_bits(values.raw_value_word(value)?);
    if !float.is_finite() {
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_INF_OR_NAN,
            message: EVAL_JSON_INF_OR_NAN_MESSAGE,
        });
        output.push(b'0');
        return Ok(());
    }
    let bytes = if float == 0.0 && float.is_sign_negative() { b"-0".to_vec() } else { values.string_bytes(value)? };
    output.extend_from_slice(&bytes);
    if flags & EVAL_JSON_PRESERVE_ZERO_FRACTION != 0
        && !bytes.iter().any(|byte| matches!(*byte, b'.' | b'e' | b'E'))
    {
        output.extend_from_slice(b".0");
    }
    Ok(())
}

/// Appends a JSON object colon, including pretty-print spacing when active.
fn eval_json_encode_append_colon(flags: i64, output: &mut Vec<u8>) {
    if flags & EVAL_JSON_PRETTY_PRINT != 0 {
        output.extend_from_slice(b": ");
    } else {
        output.push(b':');
    }
}

/// Appends PHP's four-space JSON pretty-print indentation for one nesting level.
fn eval_json_encode_pretty_indent(output: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        output.extend_from_slice(b"    ");
    }
}

/// Appends one JSON string with eval-supported PHP flag handling.
fn eval_json_encode_append_string(
    bytes: &[u8],
    flags: i64,
    position: EvalJsonStringPosition,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> EvalJsonEncodeResult {
    if flags & EVAL_JSON_NUMERIC_CHECK != 0 {
        if let Some(number) = eval_json_numeric_check_bytes(bytes) {
            output.extend_from_slice(&number);
            return Ok(());
        }
    }
    let start_len = output.len();
    output.push(b'"');
    if let Ok(value) = std::str::from_utf8(bytes) {
        for character in value.chars() {
            eval_json_encode_append_char(character, flags, output);
        }
    } else if flags & (EVAL_JSON_INVALID_UTF8_IGNORE | EVAL_JSON_INVALID_UTF8_SUBSTITUTE) == 0 {
        output.truncate(start_len);
        match position {
            EvalJsonStringPosition::Value => output.extend_from_slice(b"null"),
            EvalJsonStringPosition::Key => output.extend_from_slice(b"\"\""),
        }
        let failure = EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_UTF8,
            message: EVAL_JSON_UTF8_MESSAGE,
        };
        if matches!(position, EvalJsonStringPosition::Key) {
            // Invalid keys still allow their values and later callbacks to run.
            *error = Some(failure);
            return Ok(());
        }
        return eval_json_encode_record_error(flags, error, failure);
    } else {
        eval_json_encode_append_invalid_utf8_bytes(bytes, flags, output)?;
    }
    output.push(b'"');
    Ok(())
}

/// Appends one valid UTF-8 character using PHP JSON string escaping rules.
fn eval_json_encode_append_char(character: char, flags: i64, output: &mut Vec<u8>) {
    if character.is_ascii() {
        eval_json_encode_append_ascii_byte(character as u8, flags, output);
    } else if flags & EVAL_JSON_UNESCAPED_UNICODE != 0
        && (!matches!(character, '\u{2028}' | '\u{2029}')
            || flags & EVAL_JSON_UNESCAPED_LINE_TERMINATORS != 0)
    {
        let mut buffer = [0_u8; 4];
        output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    } else {
        eval_json_encode_append_unicode_escape(character as u32, output);
    }
}

/// Appends one ASCII byte using JSON escaping rules shared by UTF-8 and fallback paths.
fn eval_json_encode_append_ascii_byte(byte: u8, flags: i64, output: &mut Vec<u8>) {
    match byte {
        b'"' if flags & EVAL_JSON_HEX_QUOT != 0 => output.extend_from_slice(b"\\u0022"),
        b'"' => output.extend_from_slice(b"\\\""),
        b'\\' => output.extend_from_slice(b"\\\\"),
        b'/' if flags & EVAL_JSON_UNESCAPED_SLASHES == 0 => {
            output.extend_from_slice(b"\\/");
        }
        b'/' => output.push(b'/'),
        b'<' if flags & EVAL_JSON_HEX_TAG != 0 => output.extend_from_slice(b"\\u003C"),
        b'>' if flags & EVAL_JSON_HEX_TAG != 0 => output.extend_from_slice(b"\\u003E"),
        b'&' if flags & EVAL_JSON_HEX_AMP != 0 => output.extend_from_slice(b"\\u0026"),
        b'\'' if flags & EVAL_JSON_HEX_APOS != 0 => output.extend_from_slice(b"\\u0027"),
        b'\x08' => output.extend_from_slice(b"\\b"),
        b'\x0c' => output.extend_from_slice(b"\\f"),
        b'\n' => output.extend_from_slice(b"\\n"),
        b'\r' => output.extend_from_slice(b"\\r"),
        b'\t' => output.extend_from_slice(b"\\t"),
        control @ 0x00..=0x1f => {
            output.extend_from_slice(format!("\\u{control:04x}").as_bytes());
        }
        _ => output.push(byte),
    }
}

/// Appends valid scalar values as PHP JSON `\uXXXX` escapes, using surrogate pairs when needed.
fn eval_json_encode_append_unicode_escape(codepoint: u32, output: &mut Vec<u8>) {
    if codepoint <= 0xffff {
        output.extend_from_slice(format!("\\u{codepoint:04x}").as_bytes());
        return;
    }

    let codepoint = codepoint - 0x1_0000;
    let high = 0xd800 + ((codepoint >> 10) & 0x3ff);
    let low = 0xdc00 + (codepoint & 0x3ff);
    output.extend_from_slice(format!("\\u{high:04x}\\u{low:04x}").as_bytes());
}

/// Appends malformed UTF-8 bytes according to PHP's JSON invalid-UTF-8 flags.
fn eval_json_encode_append_invalid_utf8_bytes(
    mut bytes: &[u8],
    flags: i64,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(value) => {
                for character in value.chars() {
                    eval_json_encode_append_char(character, flags, output);
                }
                return Ok(());
            }
            Err(error) => {
                let valid = &bytes[..error.valid_up_to()];
                for character in std::str::from_utf8(valid)
                    .map_err(|_| EvalStatus::RuntimeFatal)?
                    .chars()
                {
                    eval_json_encode_append_char(character, flags, output);
                }
                let invalid_len = error
                    .error_len()
                    .unwrap_or(bytes.len() - valid.len())
                    .max(1);
                if flags & EVAL_JSON_INVALID_UTF8_IGNORE == 0 {
                    eval_json_encode_append_char('\u{fffd}', flags, output);
                }
                bytes = &bytes[valid.len() + invalid_len.min(bytes.len() - valid.len())..];
            }
        }
    }
    Ok(())
}

/// Returns the JSON number bytes for a PHP numeric string when `JSON_NUMERIC_CHECK` applies.
fn eval_json_numeric_check_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let value = std::str::from_utf8(bytes).ok()?.trim();
    if value.is_empty() {
        return None;
    }
    let integer_grammar = value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-'));
    if integer_grammar {
        if let Ok(integer) = value.parse::<i64>() {
            return Some(integer.to_string().into_bytes());
        }
    }
    let number = value.parse::<f64>().ok()?;
    if number.is_finite() {
        Some(number.to_string().into_bytes())
    } else {
        None
    }
}
