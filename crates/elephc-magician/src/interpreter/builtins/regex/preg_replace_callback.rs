//! Purpose:
//! Eval registry entry and implementation for `preg_replace_callback`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//!   callback invocation for `preg_replace_callback()`.

use super::super::super::*;
use super::super::*;
use super::*;

eval_builtin! {
    name: "preg_replace_callback",
    area: Regex,
    params: [pattern, callback, subject],
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

/// Replaces every regex match by invoking an eval-supported callback with `$matches`.
pub(in crate::interpreter) fn eval_preg_replace_callback_result(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_preg_replace_callback_result_from_scope(pattern, callback, subject, None, context, values)
}

/// Replaces regex matches with optional lexical scope for callback names.
fn eval_preg_replace_callback_result_from_scope(
    pattern: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
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
}


/// Dispatches by-value `preg_replace_callback()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_callback_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, callback, subject] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_preg_replace_callback_result(*pattern, *callback, *subject, context, values)
}
