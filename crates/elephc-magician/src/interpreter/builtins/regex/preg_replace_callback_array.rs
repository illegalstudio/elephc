//! Purpose:
//! Eval registry entry and implementation for `preg_replace_callback_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and the named-call router in
//!   `crate::interpreter::expressions::calls`.
//!
//! Key details:
//! - Each `pattern => callback` entry runs [`eval_preg_replace_callback_full`] over the subject
//!   the previous entry produced, as php does; a subject that fails stops the whole call with
//!   null. The compiled side builds the same loop in `src/preg_array_prelude.rs`.
//! - php validates EVERY callback before it runs any pattern.

use super::super::super::*;
use super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_replace_callback_array",
    area: Regex,
    direct: PregReplaceCallbackArray,
    values: PregReplaceCallbackArray,
}

/// Evaluates PHP `preg_replace_callback_array()` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = args.iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
    eval_builtin_preg_replace_callback_array_call(&args, context, scope, values)
}

/// Evaluates a metadata-rich call, writing its `$count` through the caller.
pub(in crate::interpreter) fn eval_builtin_preg_replace_callback_array_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, scope, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &["pattern", "subject", "limit", "count", "flags"],
            &evaluated_args,
            false,
        )?;
        let pattern = required_evaluated_ref_arg(&bound, 0)?;
        let subject = required_evaluated_ref_arg(&bound, 1)?;
        let limit = optional_evaluated_ref_arg(&bound, 2).map(|arg| arg.value);
        let flags = optional_evaluated_ref_arg(&bound, 4).map(|arg| arg.value);
        let (result, replacement_count) = eval_preg_replace_callback_array_result(
            pattern.value,
            subject.value,
            limit,
            flags,
            Some(scope),
            context,
            values,
        )?;
        let Some(count) = optional_evaluated_ref_arg(&bound, 3) else {
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

/// Applies every `pattern => callback` entry in turn and totals the replacements.
pub(in crate::interpreter) fn eval_preg_replace_callback_array_result(
    pattern: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    flags: Option<RuntimeCellHandle>,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    if !eval_preg_is_array(pattern, values)? {
        return eval_throw_type_error(
            "preg_replace_callback_array(): Argument #1 ($pattern) must be of type array",
            context,
            values,
        );
    }
    let entries = eval_preg_array_entries(pattern, values)?;
    for (_, callback) in &entries {
        if !eval_is_callable_value(*callback, lexical_scope, context, values)? {
            return eval_throw_type_error(
                "preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid \
                 callbacks",
                context,
                values,
            );
        }
    }
    let mut subject = subject;
    let mut total = 0i64;
    for (regex, callback) in entries {
        let regex = values.cast_string(regex)?;
        let (result, count) = eval_preg_replace_callback_full(
            regex,
            callback,
            subject,
            limit,
            flags,
            "preg_replace_callback_array",
            lexical_scope,
            context,
            values,
        )?;
        total += count;
        if values.is_null(result)? {
            return Ok((result, total));
        }
        subject = result;
    }
    Ok((subject, total))
}

/// Dispatches by-value `preg_replace_callback_array()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_callback_array_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, subject, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if rest.len() >= 2 {
        values.warning(
            "Warning: preg_replace_callback_array(): Argument #4 ($count) must be passed by \
             reference, value given\n",
        )?;
    }
    eval_preg_replace_callback_array_result(
        *pattern,
        *subject,
        rest.first().copied(),
        rest.get(2).copied(),
        None,
        context,
        values,
    )
    .map(|(result, _)| result)
}
