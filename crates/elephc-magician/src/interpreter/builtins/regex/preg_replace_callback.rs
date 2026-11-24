//! Purpose:
//! Eval registry entry and implementation for `preg_replace_callback`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//!   callback invocation for `preg_replace_callback()`.
//! - An invalid pattern returns PHP null. A valid PHP modifier this runtime does
//!   not implement stays an explicit error. Space, LF, and CR in the modifier
//!   suffix are ignored, and `x` is extended mode. The callback is validated
//!   before the pattern, so a missing callback is a TypeError even when the
//!   pattern is bad. A successful replacement, including the empty string, stays
//!   a string.
//! - Matching is interleaved with the callbacks, like php-src: a callback sees the error
//!   state of the matches before it, never a later match's error.

use super::super::super::*;
use super::super::*;
use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

eval_builtin! {
    contract: "preg_replace_callback",
    area: Regex,
    direct: PregReplaceCallback,
    values: PregReplaceCallback,
}

/// Evaluates PHP `preg_replace_callback()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_eval_operands(&[pattern, callback, subject], context, scope, values,
        |operands, context, scope, values| {
            eval_preg_replace_callback_result_from_scope(
                operands[0], operands[1], operands[2], Some(scope), context, values,
            )
        })
}

/// Replaces regex matches with optional lexical scope for callback names.
///
/// The callback is validated first, matching PHP: a missing callback throws
/// `TypeError` before pattern compilation. An invalid pattern then returns PHP
/// null and does not invoke the callback. A valid modifier this runtime does
/// not implement keeps the explicit runtime error. A compiled pattern returns
/// the replacement string, which may be empty.
fn eval_preg_replace_callback_result_from_scope(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    callback.with_values(context, values, |callback, context, values| {
        eval_validate_preg_replace_callback(callback, context, values)?;
        let regex = match eval_preg_callback_pattern(pattern, values)? {
            EvalPregCallbackPattern::Compiled(regex) => regex,
            EvalPregCallbackPattern::Invalid => return values.null(),
            EvalPregCallbackPattern::UnsupportedModifier => {
                return Err(EvalStatus::RuntimeFatal);
            }
        };
        let subject = values.string_bytes(subject)?;
        let mut result = Vec::with_capacity(subject.len());
        let mut cursor = 0;
        let mut matches_iter = regex.captures_iter(&subject);
        // Matches and callbacks interleave like php-src, so a callback never sees the error of
        // a later match. Each match runs against a cleared slot so its own failure is told
        // apart from a nested `preg_*` error a callback recorded; that nested error is put
        // back afterwards, staying visible without failing this call.
        let own_error = loop {
            let nested_error = eval_preg_last_error_code();
            eval_preg_set_last_error(0);
            let next = matches_iter.next();
            let match_error = eval_preg_last_error_code();
            if match_error == 0 {
                eval_preg_set_last_error(nested_error);
            }
            let Some(captures) = next else {
                break match_error;
            };
            let Some(matched) = captures.get(0) else {
                continue;
            };
            result.extend_from_slice(&subject[cursor..matched.start()]);
            let matches = eval_preg_capture_array(&subject, Some(&captures), false, false, values)?;
            let matches = RuntimeOwnerSlot::new(matches);
            let callback_result = with_runtime_owners(&matches, || {
                let result = eval_evaluated_callable_with_values(
                    &callback, vec![matches.borrowed()], context, values,
                );
                finish_eval_argument_values(result, [matches.take()], context, values)
            })?;
            with_eval_value_lease(callback_result, context, values, |callback_result, context, values| {
                let converted = values.cast_string(callback_result)?;
                with_eval_value_lease(converted, context, values, |converted, _, values| {
                    result.extend_from_slice(&values.string_bytes(converted)?);
                    Ok(())
                })
            })?;
            cursor = matched.end();
        };
        if own_error != 0 {
            eval_preg_set_last_error(own_error);
            return values.null();
        }
        result.extend_from_slice(&subject[cursor..]);
        values.string_bytes_value(&result)
    })
}


/// Dispatches by-value `preg_replace_callback()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_callback_values_result(
    evaluated_args: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_preg_replace_callback_result_from_scope(
        *pattern,
        *callback,
        *subject,
        lexical_scope,
        context,
        values,
    )
}
