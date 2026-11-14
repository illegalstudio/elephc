//! Purpose:
//! Eval registry entry and implementation for `preg_match`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and special by-ref call handling.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//! - `$matches` writeback for `preg_match()`.
use super::super::super::*;
use super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_match",
    area: Regex,
    direct: PregMatch,
    values: PregMatch,
}


/// Evaluates PHP `preg_match()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_match(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_preg_match_call(&args, context, scope, values)
}

/// Evaluates PHP `preg_match()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_preg_match_call(
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
        let flags = eval_preg_match_flags(flags, context, values)?;
        let offset = eval_preg_offset_value(offset, values)?;
        let Some(matches) = optional_evaluated_ref_arg(&bound, 2) else {
            return eval_preg_match_result(pattern.value, subject.value, offset, values);
        };
        let target = matches
            .ref_target
            .clone()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let (result, matches_array) =
            eval_preg_match_capture_result(pattern.value, subject.value, flags, offset, values)?;
        eval_write_preg_matches_target(&target, matches_array, context, values)?;
        Ok(result)
    })
}

/// Returns whether one regex matches from the PHP `$offset` without building captures.
///
/// An offset past the end returns `false`, as PHP does.
pub(in crate::interpreter) fn eval_preg_match_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    offset: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let Some(start) = eval_preg_start_offset(offset, subject.len()) else {
        return values.bool_value(false);
    };
    values.int(i64::from(regex.captures_at(&subject, start).is_some()))
}

/// Returns the match flag plus PHP `$matches` capture array for one regex search.
///
/// The search starts at the PHP `$offset` while anchors and lookbehind still see the
/// whole subject. An offset past the end returns `false` with an empty array, as PHP does.
pub(in crate::interpreter) fn eval_preg_match_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: i64,
    offset: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let Some(start) = eval_preg_start_offset(offset, subject.len()) else {
        let matches = values.array_new(0)?;
        let result = values.bool_value(false)?;
        return Ok((result, matches));
    };
    if let Some(captures) = regex.captures_at(&subject, start) {
        let matches = eval_preg_capture_array(
            &subject,
            Some(&captures),
            offset_capture,
            unmatched_as_null,
            values,
        )?;
        let matched = values.int(1)?;
        return Ok((matched, matches));
    }
    let matches =
        eval_preg_capture_array(&subject, None, offset_capture, unmatched_as_null, values)?;
    let matched = values.int(0)?;
    Ok((matched, matches))
}

/// Returns the `preg_match()` flags, raising PHP's `ValueError` for an order flag.
///
/// php-src rejects any bit of the low order byte for the non-global match and ignores
/// other unknown bits, so only the two capture-shape flags are kept.
pub(in crate::interpreter) fn eval_preg_match_flags(
    flags: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    if flags & 0xff != 0 {
        return eval_throw_builtin_value_error(
            "preg_match(): Argument #4 ($flags) must be a PREG_* constant",
            context,
            values,
        );
    }
    Ok(flags & (EVAL_PREG_OFFSET_CAPTURE | EVAL_PREG_UNMATCHED_AS_NULL))
}

/// Dispatches by-value `preg_match()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_match_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (pattern, subject, rest) = match evaluated_args {
        [pattern, subject, rest @ ..] if rest.len() <= 3 => (*pattern, *subject, rest),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    if rest.is_empty() {
        return eval_preg_match_result(pattern, subject, 0, values);
    }
    values.warning(
        "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
    )?;
    let flags = eval_preg_match_flags(rest.get(1).copied(), context, values)?;
    let offset = eval_preg_offset_value(rest.get(2).copied(), values)?;
    let (matched, matches) =
        eval_preg_match_capture_result(pattern, subject, flags, offset, values)?;
    values.release(matches)?;
    Ok(matched)
}
