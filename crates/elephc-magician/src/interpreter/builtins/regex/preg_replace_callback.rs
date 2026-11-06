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

use super::super::super::*;
use super::super::*;
use super::*;

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
    let pattern = eval_expr(pattern, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_callback_result_from_scope(pattern, callback, subject, Some(scope), context, values)
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
        for captures in regex.captures_iter(&subject) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            result.extend_from_slice(&subject[cursor..matched.start()]);
            let matches = eval_preg_capture_array(&subject, Some(&captures), false, false, values)?;
            let callback_result =
                eval_evaluated_callable_with_values(&callback, vec![matches], context, values)?;
            let callback_result = values.cast_string(callback_result)?;
            let callback_bytes = values.string_bytes(callback_result)?;
            result.extend_from_slice(&callback_bytes);
            cursor = matched.end();
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
