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

/// Resolves PHP's optional byte offset, clamping negative offsets and rejecting positive overflow.
pub(in crate::interpreter) fn eval_preg_start_offset(
    offset: Option<RuntimeCellHandle>,
    subject_len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(offset) = offset else {
        return Ok(Some(0));
    };
    let offset = eval_int_value(offset, values)?;
    if offset < 0 {
        let distance = usize::try_from(offset.unsigned_abs()).unwrap_or(usize::MAX);
        return Ok(Some(subject_len.saturating_sub(distance)));
    }
    let start = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok((start <= subject_len).then_some(start))
}

/// Builds PHP's `$matches` capture array for one regex result.
///
/// When `regex` declared no named groups this stays PHP's plain indexed
/// array (unchanged from before named-group support existed). When it did,
/// PHP's `$matches` becomes an ordered hash: each named group's own key is
/// written immediately before its numeric twin, in ascending capture-group
/// order — PCRE2's own name table decides which indices are named, so this
/// never re-derives names by parsing the pattern itself.
pub(in crate::interpreter) fn eval_preg_capture_array(
    subject: &[u8],
    regex: &Regex,
    captures: Option<&Captures<'_>>,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = captures.map_or(0, |captures| {
        eval_preg_visible_capture_len(captures, unmatched_as_null)
    });
    // A MARK verb forces the hash shape for the same reason a declared name does: `MARK` is a
    // STRING key. php appends it AFTER every capture, and reports it as a bare string even under
    // `PREG_OFFSET_CAPTURE` (measured with `php -n` 8.5), which is why it is not built through
    // `eval_preg_capture_value`.
    let mark = captures.and_then(|_| regex.last_mark());
    if regex.name_count() == 0 && mark.is_none() {
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
        return Ok(result);
    }
    let mut result = values.assoc_new(len.saturating_mul(2))?;
    if let Some(captures) = captures {
        for index in 0..len {
            let value = eval_preg_capture_value(
                subject,
                captures,
                index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            if let Some(name) = regex.group_name(index) {
                let name_key = values.string_bytes_value(&name)?;
                result = values.array_set(result, name_key, value)?;
            }
            let index_key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            result = values.array_set(result, index_key, value)?;
        }
    }
    if let Some(mark) = mark {
        let mark_key = values.string_bytes_value(b"MARK")?;
        let mark_value = values.string_bytes_value(&mark)?;
        result = values.array_set(result, mark_key, mark_value)?;
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
