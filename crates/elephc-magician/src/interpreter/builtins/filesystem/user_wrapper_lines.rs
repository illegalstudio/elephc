//! Purpose:
//! Implements line-oriented reads for eval userspace stream wrappers.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::streams` for `fgets()` and
//!   `stream_get_line()` on userspace wrapper resources.
//!
//! Key details:
//! - Reads flow through the wrapper object's `stream_read()`/`stream_eof()`
//!   methods, preserving the same wrapper state used by aggregate reads.

use super::super::super::*;
use super::user_wrapper_streams::{
    eval_user_wrapper_eof_bool, eval_user_wrapper_read_bytes,
};

/// Dispatches `fgets()` to a userspace-wrapper stream.
pub(in crate::interpreter) fn eval_user_wrapper_fgets_result(
    id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    // `fgets` keeps the newline, so an empty buffer really does mean the stream is spent.
    let Some((bytes, _)) =
        eval_user_wrapper_line_bytes(id, usize::MAX, None, true, true, context, values)?
    else {
        return values.bool_value(false).map(Some);
    };
    if bytes.is_empty() {
        return values.bool_value(false).map(Some);
    }
    values.string_bytes_value(&bytes).map(Some)
}

/// Dispatches `stream_get_line()` to a userspace-wrapper stream.
pub(in crate::interpreter) fn eval_user_wrapper_stream_get_line_result(
    id: i64,
    length: usize,
    ending: Option<&[u8]>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if context.stream_resources().user_wrapper_stream_info(id).is_none() {
        return Ok(None);
    }
    let Some((bytes, consumed)) =
        eval_user_wrapper_line_bytes(id, length, ending, false, false, context, values)?
    else {
        return values.bool_value(false).map(Some);
    };
    // A delimiter at the read position strips the segment to nothing, which PHP still
    // reports as a string. Only a stream that yielded no byte at all is false.
    if consumed == 0 {
        return values.bool_value(false).map(Some);
    }
    values.string_bytes_value(&bytes).map(Some)
}

/// Reads one wrapper line up to a limit, newline, or custom delimiter, reporting the
/// bytes read alongside how many the stream actually yielded.
///
/// The two differ whenever a stripped delimiter empties the buffer, which is the only
/// thing separating `stream_get_line`'s `""` from its `false`.
fn eval_user_wrapper_line_bytes(
    id: i64,
    length: usize,
    ending: Option<&[u8]>,
    include_ending: bool,
    stop_at_newline: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(Vec<u8>, usize)>, EvalStatus> {
    let mut output = Vec::new();
    let mut consumed = 0_usize;
    while output.len() < length {
        if eval_user_wrapper_eof_bool(id, context, values)? {
            break;
        }
        let Some(chunk) = eval_user_wrapper_read_bytes(id, 1, context, values)? else {
            return Ok(None);
        };
        if chunk.is_empty() {
            break;
        }
        output.push(chunk[0]);
        consumed += 1;
        if let Some(ending) = ending {
            if !ending.is_empty() && output.ends_with(ending) {
                if !include_ending {
                    output.truncate(output.len().saturating_sub(ending.len()));
                }
                break;
            }
        } else if stop_at_newline && chunk[0] == b'\n' {
            break;
        }
    }
    Ok(Some((output, consumed)))
}
