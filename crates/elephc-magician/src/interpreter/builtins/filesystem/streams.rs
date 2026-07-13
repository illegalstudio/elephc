//! Purpose:
//! Shared stream argument coercions for eval filesystem stream builtins.
//!
//! Called from:
//! - Leaf filesystem stream builtin files that need stream-resource coercions.
//!
//! Key details:
//! - Runtime resource payloads are zero-based while PHP-visible ids are
//!   one-based resource handles.

use super::super::super::*;
/// Converts a runtime resource cell into eval's zero-based stream id.
pub(in crate::interpreter) fn eval_stream_resource_id(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    if values.type_tag(stream)? != EVAL_TAG_RESOURCE {
        return Err(EvalStatus::RuntimeFatal);
    }
    let display_id = eval_int_value(stream, values)?;
    display_id.checked_sub(1).ok_or(EvalStatus::RuntimeFatal)
}

/// Converts a stream length argument into a non-negative `usize`.
pub(in crate::interpreter) fn eval_nonnegative_usize(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<usize, EvalStatus> {
    let value = eval_int_value(value, values)?;
    usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts an optional stream length where null and -1 mean "read all".
pub(in crate::interpreter) fn eval_optional_stream_length(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(value) = value else {
        return Ok(None);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(None);
    }
    let value = eval_int_value(value, values)?;
    if value == -1 {
        return Ok(None);
    }
    Ok(Some(
        usize::try_from(value).map_err(|_| EvalStatus::RuntimeFatal)?,
    ))
}

/// Converts an optional absolute stream offset where null and -1 mean no seek.
pub(in crate::interpreter) fn eval_optional_stream_offset(
    value: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<i64>, EvalStatus> {
    let Some(value) = value else {
        return Ok(None);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(None);
    }
    let value = eval_int_value(value, values)?;
    if value < 0 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Converts one runtime cell to a UTF-8 string for stream mode arguments.
pub(in crate::interpreter) fn eval_stream_string(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Converts an optional one-byte delimiter argument to a byte value.
pub(in crate::interpreter) fn eval_optional_delimiter(
    value: Option<RuntimeCellHandle>,
    default: u8,
    values: &mut impl RuntimeValueOps,
) -> Result<u8, EvalStatus> {
    let Some(value) = value else {
        return Ok(default);
    };
    if values.type_tag(value)? == EVAL_TAG_NULL {
        return Ok(default);
    }
    Ok(values.string_bytes(value)?.first().copied().unwrap_or(default))
}
