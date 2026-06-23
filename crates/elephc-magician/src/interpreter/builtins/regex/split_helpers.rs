//! Purpose:
//! Owns `preg_split()` flag parsing and split-piece conversion helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::split`.
//!
//! Key details:
//! - Split pieces retain byte offsets so `PREG_SPLIT_OFFSET_CAPTURE` can be applied
//!   after piece collection.

use super::super::super::*;
use super::super::*;

/// One `preg_split()` output segment plus its byte offset in the subject.
pub(in crate::interpreter) struct EvalPregSplitPiece {
    bytes: Vec<u8>,
    offset: usize,
}

/// Returns the PHP `preg_split()` limit, treating zero as unlimited.
pub(in crate::interpreter) fn eval_preg_split_limit(
    limit: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    let Some(limit) = limit else {
        return Ok(None);
    };
    let limit = eval_int_value(limit, values)?;
    if limit <= 0 {
        return Ok(None);
    }
    usize::try_from(limit)
        .map(Some)
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Returns supported `preg_split()` flags.
pub(in crate::interpreter) fn eval_preg_split_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported =
        EVAL_PREG_SPLIT_NO_EMPTY | EVAL_PREG_SPLIT_DELIM_CAPTURE | EVAL_PREG_SPLIT_OFFSET_CAPTURE;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Returns whether `preg_split()` should stop splitting and emit the remaining subject.
pub(in crate::interpreter) fn eval_preg_split_reached_limit(
    pieces: &[EvalPregSplitPiece],
    limit: Option<usize>,
) -> bool {
    matches!(limit, Some(limit) if limit > 0 && pieces.len() + 1 >= limit)
}

/// Pushes one `preg_split()` output piece, honoring `PREG_SPLIT_NO_EMPTY`.
pub(in crate::interpreter) fn eval_preg_split_push_piece(
    pieces: &mut Vec<EvalPregSplitPiece>,
    piece: &[u8],
    offset: usize,
    no_empty: bool,
) {
    if no_empty && piece.is_empty() {
        return;
    }
    pieces.push(EvalPregSplitPiece {
        bytes: piece.to_vec(),
        offset,
    });
}

/// Pushes captured delimiters for `PREG_SPLIT_DELIM_CAPTURE`.
pub(in crate::interpreter) fn eval_preg_split_push_captures(
    pieces: &mut Vec<EvalPregSplitPiece>,
    subject: &[u8],
    captures: &Captures<'_>,
    no_empty: bool,
) {
    for index in 1..captures.len() {
        if let Some(matched) = captures.get(index) {
            eval_preg_split_push_piece(
                pieces,
                &subject[matched.start()..matched.end()],
                matched.start(),
                no_empty,
            );
        }
    }
}

/// Converts one split segment to a string or PHP `[string, byte_offset]` pair.
pub(in crate::interpreter) fn eval_preg_split_piece_value(
    piece: &EvalPregSplitPiece,
    offset_capture: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = values.string_bytes_value(&piece.bytes)?;
    if !offset_capture {
        return Ok(value);
    }

    let offset = i64::try_from(piece.offset).map_err(|_| EvalStatus::RuntimeFatal)?;
    let offset = values.int(offset)?;
    let mut pair = values.array_new(2)?;
    let value_key = values.int(0)?;
    pair = values.array_set(pair, value_key, value)?;
    let offset_key = values.int(1)?;
    values.array_set(pair, offset_key, offset)
}
