//! Purpose:
//! Eval registry entry and dispatch wrappers for `json_decode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns the decoder implementation, runtime-cell materialization,
//!   direct wrapper, and by-value dispatch shape.
//! - JSON parse-error helpers live here and are reused by `json_validate`.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;
use crate::json_validate::{self, JsonParseError, JsonParseErrorKind, JsonValue};

eval_builtin! {
    name: "json_decode",
    area: Json,
    params: [
        json,
        associative = EvalBuiltinDefaultValue::Null,
        depth = EvalBuiltinDefaultValue::Int(512),
        flags = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: JsonDecode,
    values: JsonDecode,
}

/// Evaluates PHP `json_decode()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_json_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [json] => {
            let json = eval_expr(json, context, scope, values)?;
            eval_json_decode_result(json, None, None, None, context, values)
        }
        [json, associative] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            eval_json_decode_result(json, Some(associative), None, None, context, values)
        }
        [json, associative, depth] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            eval_json_decode_result(json, Some(associative), Some(depth), None, context, values)
        }
        [json, associative, depth, flags] => {
            let json = eval_expr(json, context, scope, values)?;
            let associative = eval_expr(associative, context, scope, values)?;
            let depth = eval_expr(depth, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            eval_json_decode_result(json, Some(associative), Some(depth), Some(flags), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches by-value `json_decode()` calls after argument binding.
pub(in crate::interpreter) fn eval_json_decode_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [json] => eval_json_decode_result(*json, None, None, None, context, values),
        [json, associative] => eval_json_decode_result(*json, Some(*associative), None, None, context, values),
        [json, associative, depth] => eval_json_decode_result(
            *json,
            Some(*associative),
            Some(*depth),
            None,
            context,
            values,
        ),
        [json, associative, depth, flags] => eval_json_decode_result(
            *json,
            Some(*associative),
            Some(*depth),
            Some(*flags),
            context,
            values,
        ),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Decodes one JSON string into eval runtime cells and records PHP JSON parse state.
fn eval_json_decode_result(
    json: RuntimeCellHandle,
    associative: Option<RuntimeCellHandle>,
    depth: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let flags = flags
        .map(|flags| eval_int_value(flags, values))
        .transpose()?
        .unwrap_or(0);
    let supported_flags = EVAL_JSON_BIGINT_AS_STRING
        | EVAL_JSON_INVALID_UTF8_IGNORE
        | EVAL_JSON_INVALID_UTF8_SUBSTITUTE
        | EVAL_JSON_THROW_ON_ERROR;
    if flags & !supported_flags != 0 {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    let objects_as_assoc = associative
        .map(|associative| values.truthy(associative))
        .transpose()?
        .unwrap_or(false);
    let depth = depth
        .map(|depth| eval_int_value(depth, values))
        .transpose()?
        .unwrap_or(512);
    if depth <= 0 {
        return Err(EvalStatus::RuntimeFatal);
    }

    let bytes = values.string_bytes(json)?;
    let decoded_result = if flags & EVAL_JSON_INVALID_UTF8_SUBSTITUTE != 0 {
        json_validate::decode_result_substituting_invalid_utf8(&bytes, depth as usize)
    } else if flags & EVAL_JSON_INVALID_UTF8_IGNORE != 0 {
        json_validate::decode_result_ignoring_invalid_utf8(&bytes, depth as usize)
    } else {
        json_validate::decode_result(&bytes, depth as usize)
    };
    let decoded = match decoded_result {
        Ok(decoded) => decoded,
        Err(error) => {
            let (code, message) = eval_json_parse_error_details(error, &bytes);
            if flags & EVAL_JSON_THROW_ON_ERROR != 0 {
                return eval_throw_json_exception(code, &message, context, values);
            }
            context.set_json_error(code, message);
            return values.null();
        }
    };
    context.clear_json_error();
    eval_json_decode_to_cell(decoded, flags, objects_as_assoc, values)
}

/// Materializes one parsed JSON value as an eval runtime cell.
fn eval_json_decode_to_cell(
    value: JsonValue,
    flags: i64,
    objects_as_assoc: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match value {
        JsonValue::Null => values.null(),
        JsonValue::Bool(value) => values.bool_value(value),
        JsonValue::Number(value) => eval_json_decode_number_to_cell(&value, flags, values),
        JsonValue::String(value) => values.string_bytes_value(&value),
        JsonValue::Array(elements) => {
            let mut result = values.array_new(elements.len())?;
            for (index, element) in elements.into_iter().enumerate() {
                let index = i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?;
                let key = values.int(index)?;
                let element = eval_json_decode_to_cell(element, flags, objects_as_assoc, values)?;
                result = values.array_set(result, key, element)?;
            }
            Ok(result)
        }
        JsonValue::Object(entries) => {
            if !objects_as_assoc {
                return eval_json_decode_object_to_cell(entries, flags, values);
            }
            let mut result = values.assoc_new(entries.len())?;
            for (key, value) in entries {
                let key = values.string_bytes_value(&key)?;
                let value = eval_json_decode_to_cell(value, flags, objects_as_assoc, values)?;
                result = values.array_set(result, key, value)?;
            }
            Ok(result)
        }
    }
}

/// Materializes a parsed JSON object as a `stdClass` runtime object.
fn eval_json_decode_object_to_cell(
    entries: Vec<(Vec<u8>, JsonValue)>,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let object = values.new_object("stdClass")?;
    for (key, value) in entries {
        let key = std::str::from_utf8(&key).map_err(|_| EvalStatus::RuntimeFatal)?;
        let value = eval_json_decode_to_cell(value, flags, false, values)?;
        values.property_set(object, key, value)?;
    }
    Ok(object)
}

/// Materializes one JSON number as an int when possible and as a float otherwise.
fn eval_json_decode_number_to_cell(
    value: &[u8],
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if flags & EVAL_JSON_BIGINT_AS_STRING != 0 && eval_json_number_overflows_i64(value) {
        return values.string_bytes_value(value);
    }
    let value = std::str::from_utf8(value).map_err(|_| EvalStatus::RuntimeFatal)?;
    if !value.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        if let Ok(integer) = value.parse::<i64>() {
            return values.int(integer);
        }
    }
    let float = value.parse::<f64>().map_err(|_| EvalStatus::RuntimeFatal)?;
    values.float(float)
}

/// Returns true when one integer-grammar JSON number exceeds PHP's int range.
fn eval_json_number_overflows_i64(value: &[u8]) -> bool {
    if value.iter().any(|byte| matches!(*byte, b'.' | b'e' | b'E')) {
        return false;
    }
    let (negative, digits) = if let Some(digits) = value.strip_prefix(b"-") {
        (true, digits)
    } else {
        (false, value)
    };
    let threshold = if negative {
        b"9223372036854775808".as_slice()
    } else {
        b"9223372036854775807".as_slice()
    };
    digits.len() > threshold.len() || digits.len() == threshold.len() && digits > threshold
}

/// Records one parser error into the eval-local PHP JSON error slots.
pub(super) fn eval_record_json_parse_error(
    context: &mut ElephcEvalContext,
    error: JsonParseError,
    bytes: &[u8],
) {
    let (code, message) = eval_json_parse_error_details(error, bytes);
    context.set_json_error(code, message);
}

/// Builds the PHP JSON error code and message for one parser failure.
fn eval_json_parse_error_details(error: JsonParseError, bytes: &[u8]) -> (i64, String) {
    let (code, message) = eval_json_parse_error_status(error.kind());
    let message = eval_json_error_message_with_location(message, bytes, error.offset());
    (code, message)
}

/// Creates and schedules a `JsonException` through eval's normal Throwable channel.
pub(super) fn eval_throw_json_exception(
    code: i64,
    message: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context.set_json_error(code, message.to_string());
    let exception = values.new_object("JsonException")?;
    let message = values.string(message)?;
    let code = values.int(code)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Maps eval JSON parser failures to PHP `JSON_ERROR_*` codes and messages.
fn eval_json_parse_error_status(error: JsonParseErrorKind) -> (i64, &'static str) {
    match error {
        JsonParseErrorKind::Depth => (EVAL_JSON_ERROR_DEPTH, "Maximum stack depth exceeded"),
        JsonParseErrorKind::Syntax => (EVAL_JSON_ERROR_SYNTAX, "Syntax error"),
        JsonParseErrorKind::ControlChar => (
            EVAL_JSON_ERROR_CTRL_CHAR,
            "Control character error, possibly incorrectly encoded",
        ),
        JsonParseErrorKind::Utf8 => (EVAL_JSON_ERROR_UTF8, EVAL_JSON_UTF8_MESSAGE),
        JsonParseErrorKind::Utf16 => (
            EVAL_JSON_ERROR_UTF16,
            "Single unpaired UTF-16 surrogate in unicode escape",
        ),
    }
}

/// Adds PHP's JSON line/column suffix to one base error message.
fn eval_json_error_message_with_location(message: &str, bytes: &[u8], offset: usize) -> String {
    let (line, column) = eval_json_error_location(bytes, offset);
    format!("{message} near location {line}:{column}")
}

/// Converts a zero-based JSON byte offset into PHP-style one-based line and column.
fn eval_json_error_location(bytes: &[u8], offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let offset = offset.min(bytes.len());
    for byte in &bytes[..offset] {
        if *byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
