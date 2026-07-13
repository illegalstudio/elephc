//! Purpose:
//! Converts Rust regex captures into PHP-compatible eval arrays and values.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` match and replacement modules.
//!
//! Key details:
//! - Optional offset capture uses PHP's `[string, byte_offset]` representation and
//!   unmatched captures follow `PREG_UNMATCHED_AS_NULL`.

use super::super::super::*;

/// Builds PHP's indexed `$matches` capture array for one regex result.
pub(in crate::interpreter) fn eval_preg_capture_array(
    subject: &[u8],
    captures: Option<&Captures<'_>>,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = captures.map_or(0, |captures| {
        eval_preg_visible_capture_len(captures, unmatched_as_null)
    });
    let mut result = values.array_new(len)?;
    if let Some(captures) = captures {
        for index in 0..len {
            let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                captures,
                index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            result = values.array_set(result, key, value)?;
        }
    }
    Ok(result)
}

/// Returns the capture count PHP should expose, dropping trailing unmatched groups.
pub(in crate::interpreter) fn eval_preg_visible_capture_len(
    captures: &Captures<'_>,
    unmatched_as_null: bool,
) -> usize {
    if unmatched_as_null {
        return captures.len();
    }
    let mut len = captures.len();
    while len > 1 && captures.get(len - 1).is_none() {
        len -= 1;
    }
    len
}

/// Returns one captured byte range from the original subject.
pub(in crate::interpreter) fn eval_preg_capture_bytes<'a>(
    subject: &'a [u8],
    captures: &Captures<'_>,
    index: usize,
) -> Option<&'a [u8]> {
    captures
        .get(index)
        .map(|matched| &subject[matched.start()..matched.end()])
}

/// Builds one capture entry as either a string or PHP's `[string, byte_offset]` pair.
pub(in crate::interpreter) fn eval_preg_capture_value(
    subject: &[u8],
    captures: &Captures<'_>,
    index: usize,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let matched = captures.get(index);
    let value = if matched.is_none() && unmatched_as_null {
        values.null()?
    } else {
        let bytes = matched.as_ref().map_or(b"".as_slice(), |matched| {
            &subject[matched.start()..matched.end()]
        });
        values.string_bytes_value(bytes)?
    };
    if !offset_capture {
        return Ok(value);
    }

    let offset = matched.map_or(Ok(-1_i64), |matched| {
        i64::try_from(matched.start()).map_err(|_| EvalStatus::RuntimeFatal)
    })?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}
