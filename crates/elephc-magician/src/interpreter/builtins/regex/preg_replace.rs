//! Purpose:
//! Eval registry entry and implementation for `preg_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//!   backreference replacement expansion for `preg_replace()`.

use super::super::super::*;
use super::*;

eval_builtin! {
    name: "preg_replace",
    area: Regex,
    params: [pattern, replacement, subject],
    direct: PregReplace,
    values: PregReplace,
}

/// Evaluates PHP `preg_replace()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let pattern = eval_expr(pattern, context, scope, values)?;
    let replacement = eval_expr(replacement, context, scope, values)?;
    let subject = eval_expr(subject, context, scope, values)?;
    eval_preg_replace_result(pattern, replacement, subject, values)
}

/// Replaces every regex match with a PHP-style backreference-expanded replacement.
pub(in crate::interpreter) fn eval_preg_replace_result(
    pattern: RuntimeCellHandle,
    replacement: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let regex = eval_preg_regex(pattern, values)?;
    let replacement = values.string_bytes(replacement)?;
    let subject = values.string_bytes(subject)?;
    let mut result = Vec::with_capacity(subject.len());
    let mut cursor = 0;
    for captures in regex.captures_iter(&subject) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        result.extend_from_slice(&subject[cursor..matched.start()]);
        eval_preg_expand_replacement(&replacement, &subject, &captures, &mut result);
        cursor = matched.end();
    }
    result.extend_from_slice(&subject[cursor..]);
    values.string_bytes_value(&result)
}


/// Dispatches by-value `preg_replace()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_preg_replace_result(*pattern, *replacement, *subject, values)
}
