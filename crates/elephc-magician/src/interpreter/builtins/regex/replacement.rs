//! Purpose:
//! Expands PHP preg replacement backreferences against one regex capture set.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::replace`.
//!
//! Key details:
//! - Supports `$n`, `${n}`, and `\n` capture references without allocating
//!   intermediate runtime cells.

use super::*;

/// Appends one replacement string after expanding `$n`, `${n}`, and `\n` captures.
pub(in crate::interpreter) fn eval_preg_expand_replacement(
    replacement: &[u8],
    subject: &[u8],
    captures: &Captures<'_>,
    result: &mut Vec<u8>,
) {
    let mut index = 0;
    while index < replacement.len() {
        match replacement[index] {
            b'$' => {
                if let Some((capture_index, next_index)) =
                    eval_preg_replacement_capture_index(replacement, index + 1)
                {
                    if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                        result.extend_from_slice(bytes);
                    }
                    index = next_index;
                } else {
                    result.push(replacement[index]);
                    index += 1;
                }
            }
            b'\\' if index + 1 < replacement.len() && replacement[index + 1].is_ascii_digit() => {
                let (capture_index, next_index) =
                    eval_preg_decimal_capture_index(replacement, index + 1);
                if let Some(bytes) = eval_preg_capture_bytes(subject, captures, capture_index) {
                    result.extend_from_slice(bytes);
                }
                index = next_index;
            }
            byte => {
                result.push(byte);
                index += 1;
            }
        }
    }
}

/// Parses a dollar-style replacement capture reference.
pub(in crate::interpreter) fn eval_preg_replacement_capture_index(
    bytes: &[u8],
    index: usize,
) -> Option<(usize, usize)> {
    if bytes.get(index).copied() == Some(b'{') {
        let mut cursor = index + 1;
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == start || bytes.get(cursor).copied() != Some(b'}') {
            return None;
        }
        let capture = eval_preg_decimal_bytes_to_usize(&bytes[start..cursor])?;
        return Some((capture, cursor + 1));
    }
    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        let (capture, next) = eval_preg_decimal_capture_index(bytes, index);
        return Some((capture, next));
    }
    None
}

/// Parses a one- or two-digit replacement capture reference.
pub(in crate::interpreter) fn eval_preg_decimal_capture_index(
    bytes: &[u8],
    index: usize,
) -> (usize, usize) {
    let mut cursor = index;
    let end = usize::min(bytes.len(), index + 2);
    while cursor < end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    (
        eval_preg_decimal_bytes_to_usize(&bytes[index..cursor]).unwrap_or(0),
        cursor,
    )
}

/// Converts ASCII decimal bytes into a `usize` capture index.
pub(in crate::interpreter) fn eval_preg_decimal_bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    for byte in bytes {
        value = value.checked_mul(10)?;
        value = value.checked_add(usize::from(byte - b'0'))?;
    }
    Some(value)
}
