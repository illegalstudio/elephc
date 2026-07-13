//! Purpose:
//! Eval registry entry and dispatch wrappers for `json_encode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns the encoder implementation, direct wrapper, and by-value
//!   dispatch shape.
//! - Shared `JSON_THROW_ON_ERROR` exception construction is reused from
//!   `json_decode` instead of a separate area-level helper module.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "json_encode",
    area: Json,
    params: [
        value,
        flags = EvalBuiltinDefaultValue::Int(0),
        depth = EvalBuiltinDefaultValue::Int(512),
    ],
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
    eval_json_encode_append(
        value,
        values,
        flags,
        depth as usize,
        0,
        &mut Vec::new(),
        &mut error,
        &mut output,
    )?;
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

/// Appends one JSON value to the output buffer.
fn eval_json_encode_append(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT => output.extend_from_slice(&values.string_bytes(value)?),
        EVAL_TAG_FLOAT => {
            eval_json_encode_append_float(value, values, flags, error, output)?;
        }
        EVAL_TAG_STRING => eval_json_encode_append_string(
            &values.string_bytes(value)?,
            flags,
            EvalJsonStringPosition::Value,
            error,
            output,
        )?,
        EVAL_TAG_BOOL => {
            if values.truthy(value)? {
                output.extend_from_slice(b"true");
            } else {
                output.extend_from_slice(b"false");
            }
        }
        EVAL_TAG_ARRAY => {
            eval_json_encode_append_indexed_array(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_ASSOC => {
            eval_json_encode_append_assoc(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_OBJECT => {
            eval_json_encode_append_object(
                value,
                values,
                flags,
                depth_limit,
                depth,
                arrays_seen,
                error,
                output,
            )?;
        }
        EVAL_TAG_NULL | EVAL_TAG_RESOURCE => output.extend_from_slice(b"null"),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    Ok(())
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
    let float = eval_float_value(value, values)?;
    if !float.is_finite() {
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_INF_OR_NAN,
            message: EVAL_JSON_INF_OR_NAN_MESSAGE,
        });
        output.push(b'0');
        return Ok(());
    }
    let bytes = values.string_bytes(value)?;
    output.extend_from_slice(&bytes);
    if flags & EVAL_JSON_PRESERVE_ZERO_FRACTION != 0
        && !bytes.iter().any(|byte| matches!(*byte, b'.' | b'e' | b'E'))
    {
        output.extend_from_slice(b".0");
    }
    Ok(())
}

/// Appends one indexed eval array as a JSON array or forced JSON object.
fn eval_json_encode_append_indexed_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let force_object = flags & EVAL_JSON_FORCE_OBJECT != 0;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(if force_object { b'{' } else { b'[' });
    let len = values.array_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.array_iter_key(value, position)?;
        if force_object {
            eval_json_encode_append_string(
                &values.string_bytes(key)?,
                flags & !EVAL_JSON_NUMERIC_CHECK,
                EvalJsonStringPosition::Key,
                error,
                output,
            )?;
            eval_json_encode_append_colon(flags, output);
        }
        let element = values.array_get(value, key)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(if force_object { b'}' } else { b']' });
    arrays_seen.pop();
    Ok(())
}

/// Appends one associative eval array as a JSON object.
fn eval_json_encode_append_assoc(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(b'{');
    let len = values.array_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.array_iter_key(value, position)?;
        eval_json_encode_append_string(
            &values.string_bytes(key)?,
            flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            error,
            output,
        )?;
        eval_json_encode_append_colon(flags, output);
        let element = values.array_get(value, key)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(b'}');
    arrays_seen.pop();
    Ok(())
}

/// Appends one eval runtime object as a JSON object.
fn eval_json_encode_append_object(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
    flags: i64,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
    eval_json_encode_enter_array(value, depth_limit, depth, arrays_seen)?;
    let pretty = flags & EVAL_JSON_PRETTY_PRINT != 0;
    output.push(b'{');
    let len = values.object_property_len(value)?;
    if pretty && len > 0 {
        output.push(b'\n');
    }
    for position in 0..len {
        if position > 0 {
            output.push(b',');
            if pretty {
                output.push(b'\n');
            }
        }
        if pretty {
            eval_json_encode_pretty_indent(output, depth + 1);
        }
        let key = values.object_property_iter_key(value, position)?;
        let key_bytes = values.string_bytes(key)?;
        eval_json_encode_append_string(
            &key_bytes,
            flags & !EVAL_JSON_NUMERIC_CHECK,
            EvalJsonStringPosition::Key,
            error,
            output,
        )?;
        eval_json_encode_append_colon(flags, output);
        let property = std::str::from_utf8(&key_bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
        let element = values.property_get(value, property)?;
        eval_json_encode_append(
            element,
            values,
            flags,
            depth_limit,
            depth + 1,
            arrays_seen,
            error,
            output,
        )?;
    }
    if pretty && len > 0 {
        output.push(b'\n');
        eval_json_encode_pretty_indent(output, depth);
    }
    output.push(b'}');
    arrays_seen.pop();
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

/// Records entry into one JSON array/object, rejecting depth overrun and recursion.
fn eval_json_encode_enter_array(
    value: RuntimeCellHandle,
    depth_limit: usize,
    depth: usize,
    arrays_seen: &mut Vec<usize>,
) -> Result<(), EvalStatus> {
    if depth >= depth_limit {
        return Err(EvalStatus::RuntimeFatal);
    }
    let address = value.as_ptr() as usize;
    if arrays_seen.contains(&address) {
        return Err(EvalStatus::RuntimeFatal);
    }
    arrays_seen.push(address);
    Ok(())
}

/// Appends one JSON string with eval-supported PHP flag handling.
fn eval_json_encode_append_string(
    bytes: &[u8],
    flags: i64,
    position: EvalJsonStringPosition,
    error: &mut Option<EvalJsonEncodeError>,
    output: &mut Vec<u8>,
) -> Result<(), EvalStatus> {
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
        *error = Some(EvalJsonEncodeError {
            code: EVAL_JSON_ERROR_UTF8,
            message: EVAL_JSON_UTF8_MESSAGE,
        });
        match position {
            EvalJsonStringPosition::Value => output.extend_from_slice(b"null"),
            EvalJsonStringPosition::Key => output.extend_from_slice(b"\"\""),
        }
        return Ok(());
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
    } else if flags & EVAL_JSON_UNESCAPED_UNICODE != 0 {
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
