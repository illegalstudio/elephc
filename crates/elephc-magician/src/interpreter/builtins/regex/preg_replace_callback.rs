//! Purpose:
//! Eval registry entry and implementation for `preg_replace_callback`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//! - `crate::interpreter::builtins::regex::preg_replace_callback_array`, one pattern at a time.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//!   callback invocation for `preg_replace_callback()`.
//! - Every entry point lands on [`eval_preg_replace_callback_full`], which applies php's array
//!   forms through `regex::preg_array` and hands `$flags` (`PREG_OFFSET_CAPTURE`,
//!   `PREG_UNMATCHED_AS_NULL`) to the same capture builder `preg_match()` uses.
//! - The callback is validated before the pattern, so a missing callback is a TypeError even when
//!   the pattern is bad. An invalid pattern warns and returns PHP null without invoking the
//!   callback. Space, LF, and CR in the modifier suffix are ignored, and `x` is extended mode. A
//!   successful replacement, including the empty string, stays a string.

use super::super::super::*;
use super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_replace_callback",
    area: Regex,
    direct: PregReplaceCallback,
    values: PregReplaceCallback,
}

/// php's `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL`.
const EVAL_PREG_OFFSET_CAPTURE: i64 = 256;
const EVAL_PREG_UNMATCHED_AS_NULL: i64 = 512;

/// Evaluates PHP `preg_replace_callback()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(3..=6).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pattern = eval_expr(&args[0], context, scope, values)?;
    let callback = eval_expr(&args[1], context, scope, values)?;
    let subject = eval_expr(&args[2], context, scope, values)?;
    let limit = args
        .get(3)
        .map(|limit| eval_expr(limit, context, scope, values))
        .transpose()?;
    let count_target = args
        .get(4)
        .map(|count| eval_preg_matches_target(count, context, scope, values))
        .transpose()?;
    let flags = args
        .get(5)
        .map(|flags| eval_expr(flags, context, scope, values))
        .transpose()?;
    let (result, count) = eval_preg_replace_callback_full(
        pattern,
        callback,
        subject,
        limit,
        flags,
        "preg_replace_callback",
        Some(scope),
        context,
        values,
    )?;
    if let Some(target) = count_target {
        let count = values.int(count)?;
        eval_write_preg_matches_target(&target, count, context, values)?;
    }
    Ok(result)
}

/// Evaluates a metadata-rich callback replacement call with optional counter writeback.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, scope, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &["pattern", "callback", "subject", "limit", "count", "flags"],
            &evaluated_args,
            false,
        )?;
        let pattern = required_evaluated_ref_arg(&bound, 0)?;
        let callback = required_evaluated_ref_arg(&bound, 1)?;
        let subject = required_evaluated_ref_arg(&bound, 2)?;
        let limit = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
        let flags = optional_evaluated_ref_arg(&bound, 5).map(|arg| arg.value);
        let (result, replacement_count) = eval_preg_replace_callback_full(
            pattern.value,
            callback.value,
            subject.value,
            limit,
            flags,
            "preg_replace_callback",
            Some(scope),
            context,
            values,
        )?;
        let Some(count) = optional_evaluated_ref_arg(&bound, 4) else {
            return Ok(result);
        };
        let target = count
            .ref_target
            .clone()
            .ok_or(EvalStatus::RuntimeFatal)?;
        let replacement_count = values.int(replacement_count)?;
        eval_write_preg_matches_target(&target, replacement_count, context, values)?;
        Ok(result)
    })
}

/// Replaces callback matches up to the optional limit and returns the completed count.
pub(in crate::interpreter) fn eval_preg_replace_callback_result_with_count_from_scope(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    eval_preg_replace_callback_full(
        pattern,
        callback,
        subject,
        limit,
        None,
        "preg_replace_callback",
        lexical_scope,
        context,
        values,
    )
}

/// `preg_replace_callback()` over every pattern and subject shape php accepts; `function` names
/// the builtin a malformed pattern warns under.
///
/// The callback is validated first, matching PHP: a missing callback throws `TypeError` before
/// any pattern is compiled. A rejected pattern then warns and yields null without invoking the
/// callback. The resolved callable (and the array receiver it owns) stays leased for the whole
/// replacement and is retired on every status.
#[allow(clippy::too_many_arguments)]
pub(in crate::interpreter) fn eval_preg_replace_callback_full(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    function: &str,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    let mut total = 0i64;
    let result = callback.with_values(context, values, |callback, context, values| {
        eval_validate_preg_replace_callback(callback, context, values)?;
        let limit = limit
            .map(|limit| eval_int_value(limit, values))
            .transpose()?
            .unwrap_or(-1);
        let flags = flags
            .map(|flags| eval_int_value(flags, values))
            .transpose()?
            .unwrap_or(0);
        let patterns = eval_preg_patterns(pattern, values)?;
        let (result, count) = eval_preg_over_subject(subject, false, values, |subject, values| {
            let mut result = subject;
            let mut count = 0i64;
            for regex in &patterns {
                let (next, replaced) = eval_preg_replace_callback_bytes(
                    *regex,
                    callback,
                    &result,
                    limit,
                    flags,
                    function,
                    context,
                    values,
                )?;
                count += replaced;
                let Some(next) = next else {
                    return Ok((None, count));
                };
                result = next;
            }
            Ok((Some(result), count))
        })?;
        total = count;
        Ok(result)
    })?;
    Ok((result, total))
}

/// Replaces one pattern's matches in one subject by calling `callback` with each match.
#[allow(clippy::too_many_arguments)]
fn eval_preg_replace_callback_bytes(
    pattern: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    subject: &[u8],
    limit: i64,
    flags: i64,
    function: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<Vec<u8>>, i64), EvalStatus> {
    let Some(regex) = eval_preg_regex(pattern, function, values)? else {
        return Ok((None, 0));
    };
    let offset_capture = flags & EVAL_PREG_OFFSET_CAPTURE != 0;
    let unmatched_as_null = flags & EVAL_PREG_UNMATCHED_AS_NULL != 0;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    let mut replacement_count = 0i64;
    for captures in regex.captures_iter(subject) {
        if limit >= 0 && replacement_count >= limit {
            break;
        }
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        let matches = eval_preg_capture_array(
            subject,
            &regex,
            Some(&captures),
            offset_capture,
            unmatched_as_null,
            values,
        )?;
        let callback_result =
            eval_evaluated_callable_with_values(callback, vec![matches], context, values)?;
        let callback_result = values.cast_string(callback_result)?;
        let callback_bytes = values.string_bytes(callback_result)?;
        result.extend_from_slice(&callback_bytes);
        cursor = matched.end();
        replacement_count += 1;
    }
    result.extend_from_slice(&subject[cursor..]);
    Ok((Some(result), replacement_count))
}

/// Dispatches by-value `preg_replace_callback()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_callback_values_result(
    evaluated_args: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if rest.len() >= 2 {
        values.warning(
            "Warning: preg_replace_callback(): Argument #5 ($count) must be passed by reference, \
             value given\n",
        )?;
    }
    eval_preg_replace_callback_full(
        *pattern,
        *callback,
        *subject,
        rest.first().copied(),
        rest.get(2).copied(),
        "preg_replace_callback",
        lexical_scope,
        context,
        values,
    )
    .map(|(result, _)| result)
}
