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
            &["pattern", "subject", "matches", "flags"],
            &evaluated_args,
            false,
        )?;
        let pattern = required_evaluated_ref_arg(&bound, 0)?;
        let subject = required_evaluated_ref_arg(&bound, 1)?;
        let flags = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
        let Some(matches) = optional_evaluated_ref_arg(&bound, 2) else {
            return eval_preg_match_result(pattern.value, subject.value, values);
        };
        let target = matches
            .ref_target
            .clone()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let (result, matches_array) =
            eval_preg_match_capture_result(pattern.value, subject.value, flags, values)?;
        eval_write_preg_matches_target(&target, matches_array, context, values)?;
        Ok(result)
    })
}

/// Returns whether one regex matches the subject string.
pub(in crate::interpreter) fn eval_preg_match_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    values.int(i64::from(regex.is_match(&subject)))
}

/// Returns the match flag plus PHP `$matches` capture array for one regex search.
pub(in crate::interpreter) fn eval_preg_match_capture_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let subject = values.string_bytes(subject)?;
    let flags = eval_preg_match_flags(flags, values)?;
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    if let Some(captures) = regex.captures(&subject) {
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

/// Returns supported `preg_match()` flags.
pub(in crate::interpreter) fn eval_preg_match_flags(
    flags: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let Some(flags) = flags else {
        return Ok(0);
    };
    let flags = eval_int_value(flags, values)?;
    let supported = EVAL_PREG_OFFSET_CAPTURE | EVAL_PREG_UNMATCHED_AS_NULL;
    if flags & !supported != 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(flags)
}

/// Dispatches by-value `preg_match()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_match_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [pattern, subject] => eval_preg_match_result(*pattern, *subject, values),
        [pattern, subject, _matches] => {
            values.warning(
                "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
            )?;
            let (matched, matches) =
                eval_preg_match_capture_result(*pattern, *subject, None, values)?;
            values.release(matches)?;
            Ok(matched)
        }
        [pattern, subject, _matches, flags] => {
            values.warning(
                "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
            )?;
            let (matched, matches) =
                eval_preg_match_capture_result(*pattern, *subject, Some(*flags), values)?;
            values.release(matches)?;
            Ok(matched)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
