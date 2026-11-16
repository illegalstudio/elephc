//! Purpose:
//! The array forms shared by the eval `preg_replace()` family: an array of patterns applied in
//! turn, and an array of subjects replaced one by one.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::preg_replace`
//! - `crate::interpreter::builtins::regex::preg_replace_callback`
//! - `crate::interpreter::builtins::regex::preg_filter`
//! - `crate::interpreter::builtins::regex::preg_replace_callback_array`
//!
//! Key details:
//! - The compiled side does the same in `src/preg_array_prelude.rs`; the rules below are php's
//!   (`php_pcre.c`) and both sides follow them.
//! - An array subject keeps its keys and DROPS every element whose replacement failed; a scalar
//!   subject whose replacement failed answers null. `$count` totals every replacement made,
//!   including those of a subject whose later pattern failed.
//! - `preg_filter()` additionally drops a subject no pattern matched, and answers null for a
//!   scalar one.

use super::super::super::*;

/// php's boxed runtime tags for an indexed and an associative array.
const EVAL_TAG_ARRAY: u64 = 4;
const EVAL_TAG_ASSOC: u64 = 5;

/// Returns whether a value is a PHP array, of either storage.
pub(in crate::interpreter) fn eval_preg_is_array(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))
}

/// Returns an array's key/value pairs in iteration order.
pub(in crate::interpreter) fn eval_preg_array_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<(RuntimeCellHandle, RuntimeCellHandle)>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        entries.push((key, value));
    }
    Ok(entries)
}

/// Returns a value's bytes under PHP's string conversion.
///
/// An array converts to `Array` with php's warning; `cast_string` alone answered an empty string
/// in silence, so `preg_replace('/a/', 'x', [['nested']])` replaced in `""` instead of `Array`.
pub(in crate::interpreter) fn eval_preg_string_bytes(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<u8>, EvalStatus> {
    if eval_preg_is_array(value, values)? {
        values.warning("Warning: Array to string conversion\n")?;
        return Ok(b"Array".to_vec());
    }
    let string = values.cast_string(value)?;
    let bytes = values.string_bytes(string);
    // The cast box is this helper's own; only its bytes outlive it.
    values.release(string)?;
    bytes
}

/// Releases the owner of every key/value pair read by [`eval_preg_array_entries`].
pub(in crate::interpreter) fn eval_preg_release_entries(
    entries: Vec<(RuntimeCellHandle, RuntimeCellHandle)>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut outcome = Ok(());
    for (key, value) in entries {
        for cell in [key, value] {
            let released = values.release(cell);
            if outcome.is_ok() {
                outcome = released;
            }
        }
    }
    outcome
}

/// Returns the patterns a call applies, in order: every element of an array, or the one pattern.
pub(in crate::interpreter) fn eval_preg_patterns(
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !eval_preg_is_array(pattern, values)? {
        return Ok(vec![pattern]);
    }
    let mut patterns = Vec::new();
    let entries = eval_preg_array_entries(pattern, values)?;
    for (_, element) in &entries {
        // An array element is php's `Array` with its conversion warning, like a subject's.
        let bytes = eval_preg_string_bytes(*element, values)?;
        patterns.push(values.string_bytes_value(&bytes)?);
    }
    eval_preg_release_entries(entries, values)?;
    Ok(patterns)
}

/// Releases the pattern cells [`eval_preg_patterns`] built: the casts of an array's elements.
///
/// A single pattern is answered as the caller's own cell, which stays with the caller.
pub(in crate::interpreter) fn eval_preg_release_patterns(
    patterns: Vec<RuntimeCellHandle>,
    pattern: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut outcome = Ok(());
    for cell in patterns {
        if cell == pattern {
            continue;
        }
        let released = values.release(cell);
        if outcome.is_ok() {
            outcome = released;
        }
    }
    outcome
}

/// Runs `replace_one` over one scalar subject or over every element of an array subject.
///
/// `replace_one` answers the replaced bytes, or `None` for a failed replacement, plus the number
/// of replacements it made.
pub(in crate::interpreter) fn eval_preg_over_subject<V, F>(
    subject: RuntimeCellHandle,
    filter: bool,
    values: &mut V,
    mut replace_one: F,
) -> Result<(RuntimeCellHandle, i64), EvalStatus>
where
    V: RuntimeValueOps,
    F: FnMut(Vec<u8>, &mut V) -> Result<(Option<Vec<u8>>, i64), EvalStatus>,
{
    if !eval_preg_is_array(subject, values)? {
        let bytes = eval_preg_string_bytes(subject, values)?;
        let (result, count) = replace_one(bytes, values)?;
        let result = match result {
            Some(result) if !(filter && count == 0) => values.string_bytes_value(&result)?,
            _ => values.null()?,
        };
        return Ok((result, count));
    }
    let entries = eval_preg_array_entries(subject, values)?;
    let mut out = values.assoc_new(entries.len())?;
    let mut total = 0i64;
    for (key, element) in entries {
        let bytes = eval_preg_string_bytes(element, values)?;
        let (result, count) = replace_one(bytes, values)?;
        total += count;
        if let Some(result) = result.filter(|_| !(filter && count == 0)) {
            let result = values.string_bytes_value(&result)?;
            out = values.array_set(out, key, result)?;
            // `array_set` retained the replaced string; this loop's own cell is released.
            values.release(result)?;
        }
        values.release(key)?;
        values.release(element)?;
    }
    Ok((out, total))
}
