//! Purpose:
//! Declarative eval registry entry for `explode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and evaluated-argument dispatch stay in this leaf.
//! - The optional `$limit` follows php-src: a positive limit caps the element count and lets
//!   the last element absorb the remaining suffix, `0` behaves like `1`, and a negative limit
//!   drops that many trailing segments.

eval_builtin! {
    contract: "explode",
    area: String,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

/// Evaluates PHP `explode()` over separator, string, and optional limit expressions.
pub(in crate::interpreter) fn eval_builtin_explode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (separator, string, limit) = match args {
        [separator, string] => (separator, string, None),
        [separator, string, limit] => (separator, string, Some(limit)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let separator = eval_expr(separator, context, scope, values)?;
    let string = eval_expr(string, context, scope, values)?;
    let limit = match limit {
        Some(limit) => Some(eval_expr(limit, context, scope, values)?),
        None => None,
    };
    eval_explode_result(separator, string, limit, values)
}

/// Splits one PHP byte string into an indexed array using a non-empty separator.
///
/// An omitted `$limit` means "no limit"; every other value follows php-src's rules, which are
/// resolved into a segment budget before any element is materialized.
pub(in crate::interpreter) fn eval_explode_result(
    separator: RuntimeCellHandle,
    string: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let separator = values.string_bytes(separator)?;
    if separator.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let limit = match limit {
        Some(limit) => eval_int_value(limit, values)?,
        None => i64::MAX,
    };
    let string = values.string_bytes(string)?;
    let segments = eval_explode_segments(&string, &separator);
    let (cap, extend_last) = eval_explode_element_budget(limit, segments.len() as i64);
    let mut result = EvalArrayBuilder::indexed(values, 0)?;
    if cap <= 0 {
        return Ok(result.finish());
    }
    for (index, (start, end)) in segments.iter().copied().enumerate() {
        if index as i64 >= cap {
            break;
        }
        let is_last_allowed = index as i64 + 1 == cap;
        let end = if is_last_allowed && extend_last {
            string.len()
        } else {
            end
        };
        result.index(index, |values| values.string_bytes_value(&string[start..end]))?;
    }
    Ok(result.finish())
}

/// Returns the `[start, end)` byte range of every segment a non-empty separator produces.
fn eval_explode_segments(string: &[u8], separator: &[u8]) -> Vec<(usize, usize)> {
    let mut segments = Vec::new();
    let mut start = 0;
    while let Some(found) = super::strstr::eval_find_subslice(string, separator, start) {
        segments.push((start, found));
        start = found + separator.len();
    }
    segments.push((start, string.len()));
    segments
}

/// Resolves PHP's `$limit` into an element budget plus whether the last element absorbs the tail.
///
/// A positive limit caps the element count and lets the final element run to the end of the
/// subject, `0` is treated as `1`, and a negative limit keeps `total + limit` leading segments
/// with no tail absorption.
fn eval_explode_element_budget(limit: i64, total: i64) -> (i64, bool) {
    if limit > 0 {
        (limit, true)
    } else if limit == 0 {
        (1, true)
    } else {
        (total.saturating_add(limit), false)
    }
}

/// Dispatches evaluated `explode()` calls through the builtin leaf.
pub(in crate::interpreter) fn eval_explode_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [separator, string] => eval_explode_result(*separator, *string, None, values),
        [separator, string, limit] => {
            eval_explode_result(*separator, *string, Some(*limit), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
