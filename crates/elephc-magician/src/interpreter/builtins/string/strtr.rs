//! Purpose:
//! Declarative eval registry entry and implementation for `strtr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Mirrors php-src's two shapes: `strtr($string, $from, $to)` translates bytes pairwise
//!   (truncated to the shorter list, with a later pair for the same source byte winning), and
//!   `strtr($string, $pairs)` applies replacement pairs longest-match-first in a single
//!   left-to-right pass with no re-substitution.
//! - Keys are read through their PHP string spelling, so integer keys match the same
//!   substrings php-src matches. Empty keys and keys longer than the whole subject are
//!   ignored exactly as php-src ignores them.
//! - php-src also emits `Warning: strtr(): Ignoring replacement of empty string` for a
//!   zero-length key; eval skips the key with the same observable result without warning,
//!   matching the compiled backend.

use crate::interpreter::builtins::collection_builder::{stage_collection_entry, with_collection_cells};
use std::collections::HashMap;

eval_builtin! {
    contract: "strtr",
    area: String,
    direct: Strtr,
    values: Strtr,
}

use super::super::super::*;

/// Evaluates PHP `strtr(...)` in either its pairwise or replacement-pair shape.
pub(in crate::interpreter) fn eval_builtin_strtr(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [subject, from] => {
            let subject = eval_expr(subject, context, scope, values)?;
            let from = eval_expr(from, context, scope, values)?;
            eval_strtr_result(subject, from, None, values)
        }
        [subject, from, to] => {
            let subject = eval_expr(subject, context, scope, values)?;
            let from = eval_expr(from, context, scope, values)?;
            let to = eval_expr(to, context, scope, values)?;
            eval_strtr_result(subject, from, Some(to), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies `strtr()` to already evaluated arguments, choosing the shape from `$from`.
pub(in crate::interpreter) fn eval_strtr_result(
    subject: RuntimeCellHandle,
    from: RuntimeCellHandle,
    to: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(subject)?;
    if values.is_array_like(from)? {
        let pairs = eval_strtr_pairs(from, bytes.len(), values)?;
        let output = strtr_replace_pairs(&bytes, &pairs);
        return values.string_bytes_value(&output);
    }
    let from = values.string_bytes(from)?;
    let to = match to {
        Some(to) if !values.is_null(to)? => values.string_bytes(to)?,
        _ => Vec::new(),
    };
    let output = strtr_translate_bytes(&bytes, &from, &to);
    values.string_bytes_value(&output)
}

/// Collects the usable replacement pairs from a `$pairs` array in insertion order.
///
/// Keys are read through their PHP string spelling so integer keys match the substrings
/// php-src matches. Empty keys, and keys that cannot fit inside the subject at all, are
/// skipped just as php-src skips them.
fn eval_strtr_pairs(
    from: RuntimeCellHandle,
    subject_len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<HashMap<Vec<u8>, Vec<u8>>, EvalStatus> {
    let len = values.array_len(from)?;
    let mut pairs = HashMap::with_capacity(len);
    for position in 0..len {
        with_collection_cells(values, |values, owners| {
            let (key, value) = stage_collection_entry(from, position, values, owners)?;
            let key = values.string_bytes(key)?;
            if !key.is_empty() && key.len() <= subject_len {
                pairs.insert(key, values.string_bytes(value)?);
            }
            Ok(())
        })?;
    }
    Ok(pairs)
}

/// Applies php-src's longest-match-first single pass over the subject.
fn strtr_replace_pairs(bytes: &[u8], pairs: &HashMap<Vec<u8>, Vec<u8>>) -> Vec<u8> {
    let Some(max_len) = pairs.keys().map(Vec::len).max() else {
        return bytes.to_vec();
    };
    let min_len = pairs.keys().map(Vec::len).min().unwrap_or(max_len);

    let mut output = Vec::with_capacity(bytes.len());
    let mut position = 0usize;
    while position < bytes.len() {
        let remaining = bytes.len() - position;
        let mut matched = None;
        let mut length = max_len.min(remaining);
        while length >= min_len {
            if let Some(replacement) = pairs.get(&bytes[position..position + length]) {
                matched = Some((length, replacement));
                break;
            }
            length -= 1;
        }
        match matched {
            Some((length, replacement)) => {
                output.extend_from_slice(replacement);
                position += length;
            }
            None => {
                output.push(bytes[position]);
                position += 1;
            }
        }
    }
    output
}

/// Applies php-src's pairwise byte translation, truncated to the shorter byte list.
fn strtr_translate_bytes(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut table = [0u8; 256];
    for (index, slot) in table.iter_mut().enumerate() {
        *slot = index as u8;
    }
    for index in 0..from.len().min(to.len()) {
        table[usize::from(from[index])] = to[index];
    }
    bytes
        .iter()
        .map(|byte| table[usize::from(*byte)])
        .collect::<Vec<u8>>()
}
