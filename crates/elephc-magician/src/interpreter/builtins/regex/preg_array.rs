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
    values.string_bytes(string)
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
    for (_, element) in eval_preg_array_entries(pattern, values)? {
        patterns.push(values.cast_string(element)?);
    }
    Ok(patterns)
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
        let Some(result) = result else {
            continue;
        };
        if filter && count == 0 {
            continue;
        }
        let result = values.string_bytes_value(&result)?;
        out = values.array_set(out, key, result)?;
    }
    Ok((out, total))
}
