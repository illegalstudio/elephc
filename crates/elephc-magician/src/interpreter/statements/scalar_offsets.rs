//! Purpose:
//! Preserves PHP scalar receiver behavior for local offset writes and array creation.
//!
//! Called from:
//! - `local_offset_writes` append and indexed assignment paths.
//!
//! Key details:
//! - Only null, false, or missing locals become arrays; other scalars remain intact on error.
//! - String updates return an independent byte string for COW and reference-aware writeback.

use super::*;

/// Allocates an array only for PHP's writable empty receivers after operand evaluation.
pub(super) fn eval_array_autovivification(
    current: Option<RuntimeCellHandle>, append: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(current) = current {
        match values.type_tag(current)? {
            EVAL_TAG_NULL => {}
            EVAL_TAG_BOOL if !values.truthy(current)? => {
                values.warning("Deprecated: Automatic conversion of false to array is deprecated")?;
            }
            EVAL_TAG_STRING if append => return eval_throw_error("[] operator not supported for strings", context, values),
            _ => return eval_throw_error("Cannot use a scalar value as an array", context, values),
        }
    }
    values.array_new(1)
}

/// Writes a PHP string byte, preserving the original value on errors or invalid negative offsets.
pub(super) fn eval_string_offset_write(
    current: RuntimeCellHandle, index: RuntimeCellHandle, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset = eval_string_write_offset(index, context, values)?;
    let mut bytes = values.string_bytes(current)?;
    let position = if offset < 0 { (bytes.len() as i64).checked_add(offset) } else { Some(offset) };
    let Some(position) = position.and_then(|position| usize::try_from(position).ok()) else {
        values.warning(&format!("Warning: Illegal string offset {offset}"))?;
        return values.retain(current);
    };
    let string_value = eval_string_context_value(value, context, values)?;
    let inserted = values.string_bytes(string_value);
    let released = if string_value != value { eval_release_value(context, values, string_value) } else { Ok(()) };
    let inserted = inserted.and_then(|bytes| released.map(|()| bytes))?;
    let Some(byte) = inserted.first().copied() else {
        return eval_throw_error("Cannot assign an empty string to a string offset", context, values);
    };
    if inserted.len() > 1 {
        values.warning("Warning: Only the first byte will be assigned to the string offset")?;
    }
    if position >= bytes.len() {
        let len = position.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
        bytes.try_reserve(len - bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
        bytes.resize(len, b' ');
    }
    bytes[position] = byte;
    values.string_bytes_value(&bytes)
}

/// Converts scalar string offsets using PHP's integer grammar and warning/error distinctions.
fn eval_string_write_offset(
    index: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    match values.type_tag(index)? {
        EVAL_TAG_INT => eval_int_value(index, values),
        EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_FLOAT => {
            values.warning("Warning: String offset cast occurred")?;
            let integer = values.cast_int(index)?;
            let offset = eval_int_value(integer, values);
            let released = values.release(integer);
            offset.and_then(|offset| released.map(|()| offset))
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(index)?;
            let text = String::from_utf8_lossy(&bytes);
            let trimmed = text.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{000b}');
            if let Ok(offset) = trimmed.parse::<i64>() { return Ok(offset); }
            // Integer prefixes warn; fractional or exponent prefixes are invalid even with a suffix.
            let sign = usize::from(trimmed.starts_with(['+', '-']));
            let digits = trimmed.as_bytes()[sign..].iter().take_while(|byte| byte.is_ascii_digit()).count();
            let tail = &trimmed[sign + digits..];
            let exponent = tail.strip_prefix(['e', 'E']).map(|tail| tail.strip_prefix(['+', '-']).unwrap_or(tail));
            let fractional = tail.starts_with('.') || exponent.is_some_and(|tail| tail.as_bytes().first().is_some_and(u8::is_ascii_digit));
            if digits > 0 && !fractional {
                if let Ok(offset) = trimmed[..sign + digits].parse::<i64>() {
                    values.warning(&format!("Warning: Illegal string offset \"{text}\""))?;
                    return Ok(offset);
                }
            }
            eval_throw_type_error("Cannot access offset of type string on string", context, values)
        }
        tag => {
            let name = match tag {
                EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array".to_string(),
                EVAL_TAG_OBJECT => runtime_object_class_name(index, values)?,
                EVAL_TAG_RESOURCE => "resource".to_string(),
                _ => "unknown".to_string(),
            };
            eval_throw_type_error(&format!("Cannot access offset of type {name} on string"), context, values)
        }
    }
}
