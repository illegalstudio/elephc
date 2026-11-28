//! Purpose:
//! Converts Rust regex captures into PHP-compatible eval arrays and values.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` match and replacement modules.
//!
//! Key details:
//! - Optional offset capture uses PHP's `[string, byte_offset]` representation and
//!   unmatched captures follow `PREG_UNMATCHED_AS_NULL`.
//! - `$offset` resolution for `preg_match()` / `preg_match_all()` lives here too.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

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
    let mut result = EvalArrayBuilder::indexed(values, len)?;
    if let Some(captures) = captures {
        for index in 0..len {
            result.index(index, |values| {
                eval_preg_capture_value(subject, captures, index, offset_capture, unmatched_as_null, values)
            })?;
        }
    }
    Ok(result.finish())
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

/// Builds one capture entry as either a string or PHP's [string, byte_offset] pair.
pub(in crate::interpreter) fn eval_preg_capture_value<V: RuntimeValueOps>(
    subject: &[u8],
    captures: &Captures<'_>,
    index: usize,
    offset_capture: bool,
    unmatched_as_null: bool,
    values: &mut V,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let matched = captures.get(index);
    let construct = |values: &mut V| {
        if matched.is_none() && unmatched_as_null {
            return values.null();
        }
        let bytes = matched.as_ref().map_or(b"".as_slice(), |matched| {
            &subject[matched.start()..matched.end()]
        });
        values.string_bytes_value(bytes)
    };
    if !offset_capture {
        return construct(values);
    }

    let offset = matched.as_ref().map_or(Ok(-1_i64), |matched| {
        i64::try_from(matched.start()).map_err(|_| EvalStatus::RuntimeFatal)
    })?;
    eval_preg_offset_pair(values, offset, construct)
}

/// Builds an owned value-and-offset pair for capture and split results.
pub(super) fn eval_preg_offset_pair<V: RuntimeValueOps>(
    values: &mut V,
    offset: i64,
    construct: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut pair = EvalArrayBuilder::indexed(values, 2)?;
    pair.index(0, construct)?;
    pair.index(1, |values| values.int(offset))?;
    Ok(pair.finish())
}

/// Keeps the matches array rooted until the scalar result has been constructed.
pub(super) fn eval_preg_finish_capture_result<V: RuntimeValueOps>(
    matches: RuntimeCellHandle,
    values: &mut V,
    construct: impl FnOnce(&mut V) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let matches = RuntimeOwnerSlot::new(matches);
    with_runtime_owners(&matches, || match construct(values) {
        Ok(result) => Ok((result, matches.take())),
        Err(status) => {
            let _ = values.release(matches.take());
            Err(status)
        }
    })
}

/// Reads an optional PHP `$offset` argument, defaulting to `0`.
pub(in crate::interpreter) fn eval_preg_offset_value(
    offset: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    offset.map_or(Ok(0), |offset| eval_int_value(offset, values))
}

/// Resolves a PHP `$offset` to a byte position, or `None` when it is past the end.
///
/// A negative offset counts back from the end and clamps at the subject start. An offset
/// past the end records `PREG_INTERNAL_ERROR`, as php-src does before returning `false`.
pub(in crate::interpreter) fn eval_preg_start_offset(offset: i64, subject_len: usize) -> Option<usize> {
    let start = i64::try_from(subject_len).ok().and_then(|len| {
        let start = if offset < 0 { (len + offset).max(0) } else { offset };
        (start <= len).then_some(start)
    });
    let start = start.and_then(|start| usize::try_from(start).ok());
    if start.is_none() {
        super::engine::eval_preg_set_last_error(super::engine::EVAL_PREG_INTERNAL_ERROR);
    }
    start
}
