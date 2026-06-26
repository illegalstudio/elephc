//! Purpose:
//! Implements eval support for PHP `preg_match_all()` and capture-matrix assembly.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex` re-exports.
//!
//! Key details:
//! - Pattern-order and set-order arrays share the common capture-value helper so
//!   offset and unmatched-null flags remain consistent.

use super::super::super::*;
use super::super::*;
use super::*;

/// Evaluates PHP `preg_match_all()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_match_all(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_preg_match_all_result(pattern, subject, values)
        }
        [pattern, subject, matches] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let matches_target = eval_preg_matches_target(matches, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, None, values)?;
            eval_write_preg_matches_target(&matches_target, matches_array, context, values)?;
            Ok(result)
        }
        [pattern, subject, matches, flags] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let matches_target = eval_preg_matches_target(matches, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            let (result, matches_array) =
                eval_preg_match_all_capture_result(pattern, subject, Some(flags), values)?;
            eval_write_preg_matches_target(&matches_target, matches_array, context, values)?;
            Ok(result)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Counts all non-overlapping regex matches in one subject string.
pub(in crate::interpreter) fn eval_preg_match_all_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let count = regex.captures_iter(&subject).count();
    values.int(i64::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the match count plus PHP's default `PREG_PATTERN_ORDER` `$matches` array.
pub(in crate::interpreter) fn eval_preg_match_all_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let capture_count = regex.captures_len();
    let subject = values.string_bytes(subject)?;
    let captures: Vec<Captures<'_>> = regex.captures_iter(&subject).collect();
    let count = values.int(i64::try_from(captures.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let flags = eval_preg_match_all_flags(flags, values)?;
    let matches = if flags & EVAL_PREG_SET_ORDER != 0 {
        eval_preg_match_all_set_order_array(&subject, &captures, capture_count, flags, values)?
    } else {
        eval_preg_match_all_pattern_order_array(&subject, &captures, capture_count, flags, values)?
    };
    Ok((count, matches))
}

/// Returns supported `preg_match_all()` flags.
pub(in crate::interpreter) fn eval_preg_match_all_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(EVAL_PREG_PATTERN_ORDER);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_PATTERN_ORDER
        | EVAL_PREG_SET_ORDER
        | EVAL_PREG_OFFSET_CAPTURE
        | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Builds PHP's default `preg_match_all()` pattern-order capture matrix.
pub(in crate::interpreter) fn eval_preg_match_all_pattern_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(capture_count)?;
    for capture_index in 0..capture_count {
        let mut row = values.array_new(captures.len())?;
        for (match_index, capture) in captures.iter().enumerate() {
            let key =
                values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key =
            values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}

/// Builds PHP's `preg_match_all(..., PREG_SET_ORDER)` match-order capture matrix.
pub(in crate::interpreter) fn eval_preg_match_all_set_order_array(
    subject: &[u8],
    captures: &[Captures<'_>],
    capture_count: usize,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut outer = values.array_new(captures.len())?;
    for (match_index, capture) in captures.iter().enumerate() {
        let mut row = values.array_new(capture_count)?;
        for capture_index in 0..capture_count {
            let key =
                values.int(i64::try_from(capture_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let value = eval_preg_capture_value(
                subject,
                capture,
                capture_index,
                offset_capture,
                unmatched_as_null,
                values,
            )?;
            row = values.array_set(row, key, value)?;
        }
        let key = values.int(i64::try_from(match_index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        outer = values.array_set(outer, key, row)?;
    }
    Ok(outer)
}
