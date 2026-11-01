//! Purpose:
//! Eval registry entry and implementation for `preg_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//! - `crate::interpreter::builtins::regex::preg_filter`, which is this with `filter` set.
//!
//! Key details:
//! - This file owns registry metadata, direct dispatch, by-value dispatch, and
//!   backreference replacement expansion for `preg_replace()`.
//! - Every entry point binds its operands and lands on [`eval_preg_replace_full`], which applies
//!   php's array forms through `regex::preg_array`.

use super::super::super::*;
use super::*;

eval_builtin! {
    contract: "preg_replace",
    area: Regex,
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
    if !(3..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let pattern = eval_expr(&args[0], context, scope, values)?;
    let replacement = eval_expr(&args[1], context, scope, values)?;
    let subject = eval_expr(&args[2], context, scope, values)?;
    let limit = args
        .get(3)
        .map(|limit| eval_expr(limit, context, scope, values))
        .transpose()?;
    let count_target = args
        .get(4)
        .map(|count| eval_preg_matches_target(count, context, scope, values))
        .transpose()?;
    let (result, count) = eval_preg_replace_full(
        pattern,
        replacement,
        subject,
        limit,
        false,
        "preg_replace",
        context,
        values,
    )?;
    if let Some(target) = count_target {
        let count = values.int(count)?;
        eval_write_preg_matches_target(&target, count, context, values)?;
    }
    Ok(result)
}

/// Evaluates a metadata-rich call while preserving the optional counter destination.
pub(in crate::interpreter) fn eval_builtin_preg_replace_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_bound_preg_replace_call(args, false, "preg_replace", context, scope, values)
}

/// Binds `preg_replace()`/`preg_filter()` arguments, runs the replacement and writes `$count`.
pub(in crate::interpreter) fn eval_bound_preg_replace_call(
    args: &[EvalCallArg],
    filter: bool,
    function: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |evaluated_args, context, _, values| {
        let (bound, _) = bind_evaluated_ref_builtin_args(
            &["pattern", "replacement", "subject", "limit", "count"],
            &evaluated_args,
            false,
        )?;
        let pattern = required_evaluated_ref_arg(&bound, 0)?;
        let replacement = required_evaluated_ref_arg(&bound, 1)?;
        let subject = required_evaluated_ref_arg(&bound, 2)?;
        let limit = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
        let (result, replacement_count) = eval_preg_replace_full(
            pattern.value,
            replacement.value,
            subject.value,
            limit,
            filter,
            function,
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

/// Replaces regex matches up to the optional limit and returns the replacement count.
pub(in crate::interpreter) fn eval_preg_replace_result_with_count(
    pattern: RuntimeCellHandle,
    replacement: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    eval_preg_replace_full(
        pattern,
        replacement,
        subject,
        limit,
        false,
        "preg_replace",
        context,
        values,
    )
}

/// `preg_replace()` and `preg_filter()`, over every pattern and subject shape php accepts.
///
/// A replacement array is taken POSITIONALLY in the patterns' iteration order -- its keys are
/// ignored -- and runs out as `''`. It is a `TypeError` against a single pattern.
#[allow(clippy::too_many_arguments)]
pub(in crate::interpreter) fn eval_preg_replace_full(
    pattern: RuntimeCellHandle,
    replacement: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    limit: Option<RuntimeCellHandle>,
    filter: bool,
    function: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, i64), EvalStatus> {
    let pattern_is_array = eval_preg_is_array(pattern, values)?;
    let replacement_is_array = eval_preg_is_array(replacement, values)?;
    if replacement_is_array && !pattern_is_array {
        return eval_throw_type_error(
            &format!(
                "{function}(): Argument #1 ($pattern) must be of type array when argument #2 \
                 ($replacement) is an array, string given"
            ),
            context,
            values,
        );
    }
    let limit = limit
        .map(|limit| eval_int_value(limit, values))
        .transpose()?
        .unwrap_or(-1);
    let patterns = eval_preg_patterns(pattern, values)?;
    let mut replacements = Vec::with_capacity(patterns.len());
    if replacement_is_array {
        for (_, element) in eval_preg_array_entries(replacement, values)? {
            replacements.push(eval_preg_string_bytes(element, values)?);
        }
    } else {
        let bytes = eval_preg_string_bytes(replacement, values)?;
        replacements.resize(patterns.len(), bytes);
    }
    eval_preg_over_subject(subject, filter, values, |subject, values| {
        let mut result = subject;
        let mut count = 0i64;
        for (index, regex) in patterns.iter().enumerate() {
            let current = replacements.get(index).map(Vec::as_slice).unwrap_or(&[]);
            let (next, replaced) =
                eval_preg_replace_bytes(*regex, current, &result, limit, function, values)?;
            count += replaced;
            let Some(next) = next else {
                return Ok((None, count));
            };
            result = next;
        }
        Ok((Some(result), count))
    })
}

/// Replaces one pattern in one subject; `None` is a pattern php rejects or a failed match.
pub(in crate::interpreter) fn eval_preg_replace_bytes(
    pattern: RuntimeCellHandle,
    replacement: &[u8],
    subject: &[u8],
    limit: i64,
    function: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<(Option<Vec<u8>>, i64), EvalStatus> {
    let Some(regex) = eval_preg_regex(pattern, function, values)? else {
        return Ok((None, 0));
    };
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
        eval_preg_expand_replacement(replacement, subject, &captures, &mut result);
        cursor = matched.end();
        replacement_count += 1;
    }
    if crate::regex_provider::regex_last_error() != 0 {
        // A failed match (a subject that is not valid UTF-8 under `u`) makes the result null.
        return Ok((None, replacement_count));
    }
    result.extend_from_slice(&subject[cursor..]);
    Ok((Some(result), replacement_count))
}

/// Dispatches by-value `preg_replace()` calls after argument binding.
pub(in crate::interpreter) fn eval_preg_replace_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_preg_replace_by_value("preg_replace", false, evaluated_args, context, values)
}

/// Runs a by-value `preg_replace()`/`preg_filter()` call; a by-value `$count` is only warned about.
pub(in crate::interpreter) fn eval_preg_replace_by_value(
    function: &str,
    filter: bool,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [pattern, replacement, subject, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    if rest.len() == 2 {
        values.warning(&format!(
            "Warning: {function}(): Argument #5 ($count) must be passed by reference, value \
             given\n"
        ))?;
    }
    eval_preg_replace_full(
        *pattern,
        *replacement,
        *subject,
        rest.first().copied(),
        filter,
        function,
        context,
        values,
    )
    .map(|(result, _)| result)
}
