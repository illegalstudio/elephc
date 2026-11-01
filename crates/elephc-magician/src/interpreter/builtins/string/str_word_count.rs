//! Purpose:
//! Declarative eval registry entry and implementation for `str_word_count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src's word definition exactly: C-locale `isalpha()` plus `'` and `-`,
//!   extended by every byte of the optional `$characters` list, with a leading `'`/`-` and a
//!   trailing `-` dropped unless the character list re-admits them.
//! - Format `0` returns the word count, `1` the list of words, and `2` the byte-offset map.
//!   Any other format is php-src's catchable `ValueError`, raised through eval's
//!   pending-throw state so `catch (ValueError $e)` behaves as it does under the compiler.

eval_builtin! {
    contract: "str_word_count",
    area: String,
    direct: StrWordCount,
    values: StrWordCount,
}

use super::super::super::*;

/// Evaluates PHP `str_word_count(...)` over one subject and its optional format/characters.
pub(in crate::interpreter) fn eval_builtin_str_word_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [subject] => {
            let subject = eval_expr(subject, context, scope, values)?;
            eval_str_word_count_result(subject, None, None, context, values)
        }
        [subject, format] => {
            let subject = eval_expr(subject, context, scope, values)?;
            let format = eval_expr(format, context, scope, values)?;
            eval_str_word_count_result(subject, Some(format), None, context, values)
        }
        [subject, format, characters] => {
            let subject = eval_expr(subject, context, scope, values)?;
            let format = eval_expr(format, context, scope, values)?;
            let characters = eval_expr(characters, context, scope, values)?;
            eval_str_word_count_result(subject, Some(format), Some(characters), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Scans an already evaluated subject and materializes the requested `$format` result.
pub(in crate::interpreter) fn eval_str_word_count_result(
    subject: RuntimeCellHandle,
    format: Option<RuntimeCellHandle>,
    characters: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(subject)?;
    let format = match format {
        Some(format) => eval_int_value(format, values)?,
        None => 0,
    };
    if !(0..=2).contains(&format) {
        return eval_str_word_count_format_error(context, values);
    }
    let mut mask = [false; 256];
    if let Some(characters) = characters {
        if !values.is_null(characters)? {
            for byte in values.string_bytes(characters)? {
                mask[byte as usize] = true;
            }
        }
    }

    let words = str_word_count_words(&bytes, &mask);
    if format == 0 {
        let count = i64::try_from(words.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
        return values.int(count);
    }
    // Format 2 keys words by their byte offset, which is sparse: an indexed array would pad
    // every skipped offset with an empty element, so the map is built as an associative array.
    let mut result = if format == 1 {
        values.array_new(words.len())?
    } else {
        values.assoc_new(words.len())?
    };
    for (index, (offset, word)) in words.iter().enumerate() {
        let key = match format {
            1 => i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?,
            _ => i64::try_from(*offset).map_err(|_| EvalStatus::RuntimeFatal)?,
        };
        let key = values.int(key)?;
        let value = values.string_bytes_value(word)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Returns each `(byte offset, word bytes)` pair php-src's `str_word_count()` would emit.
///
/// The leading `'`/`-` and trailing `-` trims run before the scan, exactly as php-src does,
/// and a candidate that covers zero bytes is a separator rather than a word.
fn str_word_count_words(bytes: &[u8], mask: &[bool; 256]) -> Vec<(usize, Vec<u8>)> {
    let mut words = Vec::new();
    if bytes.is_empty() {
        return words;
    }
    let mut start = 0usize;
    let mut end = bytes.len();
    if (bytes[0] == b'\'' && !mask[usize::from(b'\'')])
        || (bytes[0] == b'-' && !mask[usize::from(b'-')])
    {
        start += 1;
    }
    if bytes[end - 1] == b'-' && !mask[usize::from(b'-')] {
        end -= 1;
    }

    let mut position = start;
    while position < end {
        let word_start = position;
        while position < end && str_word_count_is_word_byte(bytes[position], mask) {
            position += 1;
        }
        if position > word_start {
            words.push((word_start, bytes[word_start..position].to_vec()));
        }
        position += 1;
    }
    words
}

/// Returns whether one byte continues a php-src `str_word_count()` word.
fn str_word_count_is_word_byte(byte: u8, mask: &[bool; 256]) -> bool {
    byte.is_ascii_alphabetic() || mask[usize::from(byte)] || byte == b'\'' || byte == b'-'
}

/// Raises PHP's catchable `ValueError` for a `$format` outside `0..=2`.
fn eval_str_word_count_format_error<T>(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_builtin_exception_with_code("ValueError", "str_word_count(): Argument #2 ($format) must be a valid format value", 0, context, values)
}
