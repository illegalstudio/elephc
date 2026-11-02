//! Purpose:
//! Eval registry entry and implementation for `preg_match_all`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and special by-ref call handling.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//! - capture-matrix assembly for `preg_match_all()`.
use super::super::super::*;
use super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_match_all",
    area: Regex,
    direct: PregMatchAll,
    values: PregMatchAll,
}


/// Evaluates PHP `preg_match_all()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_match_all(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_preg_match_all_call(&args, context, scope, values)
}

/// Evaluates PHP `preg_match_all()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_preg_match_all_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &["pattern", "subject", "matches", "flags", "offset"],
            &evaluated_args,
            false,
        )?;
        let pattern = required_evaluated_ref_arg(&bound, 0)?;
        let subject = required_evaluated_ref_arg(&bound, 1)?;
        let flags = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
        let offset = optional_evaluated_ref_arg(&bound, 4).map(|arg| arg.value);
        let Some(matches) = optional_evaluated_ref_arg(&bound, 2) else {
            eval_preg_match_all_require_supported_flags(flags, context, values)?;
            return eval_preg_match_all_result_at(pattern.value, subject.value, offset, values);
        };
        let target = matches
            .ref_target
            .clone()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let flags = eval_preg_match_all_require_supported_flags(flags, context, values)?;
        eval_preg_define_matches_target(&target, matches.value, context, values)?;
        let (result, matches_array) =
            eval_preg_match_all_capture_result(pattern.value, subject.value, flags, offset, values)?;
        if let Some(matches_array) = matches_array {
            eval_write_preg_matches_target(&target, matches_array, context, values)?;
        }
        Ok(result)
    })
}

/// Counts all non-overlapping regex matches in one subject string.
pub(in crate::interpreter) fn eval_preg_match_all_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_preg_match_all_result_at(pattern, subject, None, values)
}

/// Counts all non-overlapping regex matches beginning at PHP's optional byte offset.
pub(in crate::interpreter) fn eval_preg_match_all_result_at(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    offset: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(regex) = eval_preg_regex(pattern, "preg_match_all", values)? else {
        return values.bool_value(false);
    };
    let subject = values.string_bytes(subject)?;
    let Some(offset) = eval_preg_start_offset(offset, subject.len(), values)? else {
        return values.bool_value(false);
    };
    let count = regex.captures_iter_at(&subject, offset).count();
    values.int(i64::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the match count plus PHP's default `PREG_PATTERN_ORDER` `$matches` array.
pub(in crate::interpreter) fn eval_preg_match_all_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: i64,
    offset: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<RuntimeCellHandle>), EvalStatus> {
    let Some(regex) = eval_preg_regex(pattern, "preg_match_all", values)? else {
        return Ok((values.bool_value(false)?, None));
    };
    let capture_count = regex.captures_len();
    let subject = values.string_bytes(subject)?;
    let Some(offset) = eval_preg_start_offset(offset, subject.len(), values)? else {
        return Ok((values.bool_value(false)?, Some(values.array_new(0)?)));
    };
    let captures: Vec<Captures<'_>> = regex.captures_iter_at(&subject, offset).collect();
    let count = values.int(i64::try_from(captures.len()).map_err(|_| EvalStatus::RuntimeFatal)?)?;
    let matches = if flags & EVAL_PREG_SET_ORDER != 0 {
        eval_preg_match_all_set_order_array(&subject, &captures, capture_count, flags, values)?
    } else {
        eval_preg_match_all_pattern_order_array(&subject, &captures, capture_count, flags, values)?
    };
    Ok((count, Some(matches)))
}

/// Reads the flag value before selecting the count-only or capture path.
pub(in crate::interpreter) fn eval_preg_match_all_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(EVAL_PREG_PATTERN_ORDER);
    };
    eval_int_value(flags, values)
}

/// Validates an eval `preg_match_all()` flag set, raising PHP's exact `ValueError`.
pub(in crate::interpreter) fn eval_preg_match_all_require_supported_flags(
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let flags = eval_preg_match_all_flags(flags, values)?;
    if !eval_preg_match_all_flags_supported(flags) {
        return eval_throw_builtin_value_error(
            "preg_match_all(): Argument #4 ($flags) must be a PREG_* constant",
            context,
            values,
        );
    }
    Ok(flags)
}

/// Recognizes the `PREG_*` flag combinations PHP accepts for `preg_match_all()`.
fn eval_preg_match_all_flags_supported(flags: i64) -> bool {
    let supported = EVAL_PREG_PATTERN_ORDER
        | EVAL_PREG_SET_ORDER
        | EVAL_PREG_OFFSET_CAPTURE
        | EVAL_PREG_UNMATCHED_AS_NULL;
    flags & !supported == 0
        && flags & (EVAL_PREG_PATTERN_ORDER | EVAL_PREG_SET_ORDER)
            != (EVAL_PREG_PATTERN_ORDER | EVAL_PREG_SET_ORDER)
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
        // php's SET_ORDER rows drop trailing groups that did not participate in THAT match
        // (`"ab, cd=5"` against `([a-z]+)(?:=([0-9]+))?` gives `["ab","ab"]` for the first and
        // `["cd=5","cd","5"]` for the second), unless UNMATCHED_AS_NULL asks for every group.
        // PATTERN_ORDER fills every column, so only this builder trims.
        let row_len = capture_count.min(eval_preg_visible_capture_len(capture, unmatched_as_null));
        let mut row = values.array_new(row_len)?;
        for capture_index in 0..row_len {
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

/// Dispatches by-value `preg_match_all()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_match_all_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [pattern, subject] => eval_preg_match_all_result(*pattern, *subject, values),
        [pattern, subject, _matches] => {
            values.warning(
                "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
            )?;
            let (count, matches) = eval_preg_match_all_capture_result(
                *pattern,
                *subject,
                EVAL_PREG_PATTERN_ORDER,
                None,
                values,
            )?;
            if let Some(matches) = matches {
                values.release(matches)?;
            }
            Ok(count)
        }
        [pattern, subject, _matches, flags] => {
            values.warning(
                "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
            )?;
            let flags = eval_preg_match_all_require_supported_flags(Some(*flags), context, values)?;
            let (count, matches) =
                eval_preg_match_all_capture_result(*pattern, *subject, flags, None, values)?;
            if let Some(matches) = matches {
                values.release(matches)?;
            }
            Ok(count)
        }
        [pattern, subject, _matches, flags, offset] => {
            values.warning(
                "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
            )?;
            let flags = eval_preg_match_all_require_supported_flags(Some(*flags), context, values)?;
            let (count, matches) = eval_preg_match_all_capture_result(
                *pattern,
                *subject,
                flags,
                Some(*offset),
                values,
            )?;
            if let Some(matches) = matches {
                values.release(matches)?;
            }
            Ok(count)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
