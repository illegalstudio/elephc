//! Purpose:
//! Eval registry entry and implementation for `mb_ereg_match`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` direct and by-value dispatch.
//!
//! Key details:
//! - `mb_ereg_match($pattern, $string, $options = null)` is a start-anchored match:
//!   the pattern is a raw mbregex body (no preg delimiters), and a successful match
//!   counts only when it begins at byte offset 0 — mirroring the AOT runtime helper,
//!   which enforces `rm_so == 0` on PCRE2's leftmost match.
//! - The `$options` string maps `i` to case-insensitive compilation; other option
//!   bytes are accepted without additional runtime effect, matching the AOT helper.

use super::super::super::*;
use super::super::spec::EvalBuiltinDefaultValue;
use super::*;

eval_builtin! {
    name: "mb_ereg_match",
    area: Regex,
    params: [pattern, subject, options = EvalBuiltinDefaultValue::Null],
    direct: MbEregMatch,
    values: MbEregMatch,
}

/// Evaluates PHP `mb_ereg_match()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_mb_ereg_match(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [pattern, subject] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            eval_mb_ereg_match_result(pattern, subject, None, values)
        }
        [pattern, subject, options] => {
            let pattern = eval_expr(pattern, context, scope, values)?;
            let subject = eval_expr(subject, context, scope, values)?;
            let options = eval_expr(options, context, scope, values)?;
            eval_mb_ereg_match_result(pattern, subject, Some(options), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches by-value `mb_ereg_match()` calls after argument binding.
pub(in crate::interpreter) fn eval_mb_ereg_match_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [pattern, subject] => eval_mb_ereg_match_result(*pattern, *subject, None, values),
        [pattern, subject, options] => {
            eval_mb_ereg_match_result(*pattern, *subject, Some(*options), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns whether the raw mbregex pattern matches the subject start.
pub(in crate::interpreter) fn eval_mb_ereg_match_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let modifiers = eval_mb_ereg_match_modifiers(options, values)?;
    let pattern = values.string_bytes(pattern)?;
    let regex = Regex::compile(&pattern, modifiers)?;
    let subject = values.string_bytes(subject)?;
    let matched = regex
        .captures(&subject)
        .and_then(|captures| captures.get(0))
        .is_some_and(|full_match| full_match.start() == 0);
    values.bool_value(matched)
}

/// Translates the optional `mb_ereg_match()` options string into compile modifiers.
fn eval_mb_ereg_match_modifiers(
    options: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalPregModifiers, EvalStatus> {
    let Some(options) = options else {
        return Ok(EvalPregModifiers::default());
    };
    if values.is_null(options)? {
        return Ok(EvalPregModifiers::default());
    }
    let options = values.string_bytes(options)?;
    Ok(EvalPregModifiers {
        case_insensitive: options.contains(&b'i'),
        ..EvalPregModifiers::default()
    })
}
