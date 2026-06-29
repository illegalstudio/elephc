//! Purpose:
//! Dispatches dynamic callable invocations of builtin mutators while preserving
//! caller-side by-reference targets captured during argument evaluation.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::callable`.
//!
//! Key details:
//! - This module only handles builtin calls whose direct PHP semantics can write
//!   to caller storage. Other builtins continue through the by-value dispatcher.

use super::super::super::*;
use super::super::*;

/// Dispatches dynamic builtin calls that must preserve by-reference caller targets.
pub(in crate::interpreter) fn eval_mutating_builtin_with_call_array_args(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "settype" => eval_dynamic_settype_call(evaluated_args, context, values)?,
        "array_walk" => eval_dynamic_array_walk_call(evaluated_args, context, values)?,
        "array_pop" | "array_shift" => {
            eval_dynamic_array_pop_shift_call(name, evaluated_args, context, values)?
        }
        "array_push" | "array_unshift" => {
            eval_dynamic_array_push_unshift_call(name, evaluated_args, context, values)?
        }
        "array_splice" => eval_dynamic_array_splice_call(evaluated_args, context, values)?,
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            eval_dynamic_array_sort_call(name, evaluated_args, context, values)?
        }
        "uasort" | "uksort" | "usort" => {
            eval_dynamic_user_sort_call(name, evaluated_args, context, values)?
        }
        "preg_match" => eval_dynamic_preg_match_call(evaluated_args, context, values)?,
        "preg_match_all" => eval_dynamic_preg_match_all_call(evaluated_args, context, values)?,
        "flock" => eval_dynamic_flock_call(evaluated_args, context, values)?,
        _ => return Ok(None),
    };
    Ok(result)
}

/// Evaluates a dynamic `settype()` call when the first argument is writable.
fn eval_dynamic_settype_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(&["var", "type"], evaluated_args, false)?;
    let var = required_evaluated_ref_arg(&bound, 0)?;
    let type_name = required_evaluated_ref_arg(&bound, 1)?;
    let Some(target) = var.ref_target.as_ref() else {
        return Ok(None);
    };
    let Some(converted) = eval_settype_cast_value(var.value, type_name.value, values)? else {
        return values.bool_value(false).map(Some);
    };
    eval_write_direct_ref_target(
        target,
        converted,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )?;
    values.bool_value(true).map(Some)
}

/// Evaluates a dynamic `array_walk()` call when the array argument is writable.
fn eval_dynamic_array_walk_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) =
        bind_evaluated_ref_builtin_args(&["array", "callback"], evaluated_args, false)?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let callback = required_evaluated_ref_arg(&bound, 1)?;
    let Some(target) = array.ref_target.clone() else {
        return Ok(None);
    };
    eval_array_walk_ref_result(array.value, target, callback.value, context, values).map(Some)
}

/// Evaluates a dynamic `array_pop()` or `array_shift()` call against a writable array.
fn eval_dynamic_array_pop_shift_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(&["array"], evaluated_args, false)?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let Some(target) = array.ref_target.as_ref() else {
        return Ok(None);
    };
    let (result, replacement) = eval_array_pop_shift_replacement(name, array.value, values)?;
    eval_write_direct_ref_target(target, replacement, context, values, None)?;
    Ok(Some(result))
}

/// Evaluates dynamic `array_push()` or `array_unshift()` calls against a writable array.
fn eval_dynamic_array_push_unshift_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, inserted) =
        bind_evaluated_ref_builtin_args(&["array", "values"], evaluated_args, true)?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let Some(target) = array.ref_target.as_ref() else {
        return Ok(None);
    };
    if inserted.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let inserted_values = inserted.iter().map(|arg| arg.value).collect::<Vec<_>>();
    let replacement =
        eval_array_push_unshift_replacement(name, array.value, &inserted_values, values)?;
    let result = eval_array_push_unshift_count_result(array.value, inserted_values.len(), values)?;
    eval_write_direct_ref_target(target, replacement, context, values, None)?;
    Ok(Some(result))
}

/// Evaluates a dynamic `array_splice()` call against a writable array.
fn eval_dynamic_array_splice_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["array", "offset", "length", "replacement"],
        evaluated_args,
        false,
    )?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let offset = required_evaluated_ref_arg(&bound, 1)?;
    let Some(target) = array.ref_target.as_ref() else {
        return Ok(None);
    };
    let length = optional_evaluated_ref_arg(&bound, 2).map(|arg| arg.value);
    let replacement_arg = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
    let (removed, replacement) = eval_array_splice_removed_and_replacement(
        array.value,
        offset.value,
        length,
        replacement_arg,
        values,
    )?;
    eval_write_direct_ref_target(target, replacement, context, values, None)?;
    Ok(Some(removed))
}

/// Evaluates a dynamic standard array sort call against a writable array.
fn eval_dynamic_array_sort_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(&["array"], evaluated_args, false)?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let Some(target) = array.ref_target.as_ref() else {
        return Ok(None);
    };
    let replacement = eval_array_sort_replacement(name, array.value, values)?;
    let result = values.bool_value(true)?;
    eval_write_direct_ref_target(target, replacement, context, values, None)?;
    Ok(Some(result))
}

/// Evaluates a dynamic user-comparator sort call against a writable array.
fn eval_dynamic_user_sort_call(
    name: &str,
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) =
        bind_evaluated_ref_builtin_args(&["array", "callback"], evaluated_args, false)?;
    let array = required_evaluated_ref_arg(&bound, 0)?;
    let callback = required_evaluated_ref_arg(&bound, 1)?;
    let Some(target) = array.ref_target.as_ref() else {
        return Ok(None);
    };
    let replacement =
        eval_user_sort_replacement(name, array.value, callback.value, context, values)?;
    let result = values.bool_value(true)?;
    eval_write_direct_ref_target(target, replacement, context, values, None)?;
    Ok(Some(result))
}

/// Evaluates a dynamic `preg_match()` call when `$matches` is a writable lvalue.
fn eval_dynamic_preg_match_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["pattern", "subject", "matches", "flags"],
        evaluated_args,
        false,
    )?;
    let pattern = required_evaluated_ref_arg(&bound, 0)?;
    let subject = required_evaluated_ref_arg(&bound, 1)?;
    let Some(matches) = optional_evaluated_ref_arg(&bound, 2) else {
        return Ok(None);
    };
    let Some(target) = matches.ref_target.as_ref() else {
        return Ok(None);
    };
    let flags = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
    let (result, matches_array) =
        eval_preg_match_capture_result(pattern.value, subject.value, flags, values)?;
    eval_write_preg_matches_target(target, matches_array, context, values)?;
    Ok(Some(result))
}

/// Evaluates a dynamic `preg_match_all()` call when `$matches` is a writable lvalue.
fn eval_dynamic_preg_match_all_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["pattern", "subject", "matches", "flags"],
        evaluated_args,
        false,
    )?;
    let pattern = required_evaluated_ref_arg(&bound, 0)?;
    let subject = required_evaluated_ref_arg(&bound, 1)?;
    let Some(matches) = optional_evaluated_ref_arg(&bound, 2) else {
        return Ok(None);
    };
    let Some(target) = matches.ref_target.as_ref() else {
        return Ok(None);
    };
    let flags = optional_evaluated_ref_arg(&bound, 3).map(|arg| arg.value);
    let (result, matches_array) =
        eval_preg_match_all_capture_result(pattern.value, subject.value, flags, values)?;
    eval_write_preg_matches_target(target, matches_array, context, values)?;
    Ok(Some(result))
}

/// Evaluates a dynamic `flock()` call when `$would_block` is a writable lvalue.
fn eval_dynamic_flock_call(
    evaluated_args: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["stream", "operation", "would_block"],
        evaluated_args,
        false,
    )?;
    let stream = required_evaluated_ref_arg(&bound, 0)?;
    let operation = required_evaluated_ref_arg(&bound, 1)?;
    let Some(would_block) = optional_evaluated_ref_arg(&bound, 2) else {
        return Ok(None);
    };
    let Some(target) = would_block.ref_target.as_ref() else {
        return Ok(None);
    };
    let (success, would_block) =
        eval_flock_result(stream.value, operation.value, context, values)?;
    let would_block = values.bool_value(would_block)?;
    eval_write_direct_ref_target(
        target,
        would_block,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )?;
    values.bool_value(success).map(Some)
}

/// Binds already evaluated arguments while preserving by-reference target metadata.
fn bind_evaluated_ref_builtin_args(
    params: &[&str],
    evaluated_args: &[EvaluatedCallArg],
    variadic_last: bool,
) -> Result<(Vec<Option<EvaluatedCallArg>>, Vec<EvaluatedCallArg>), EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut variadic_args = Vec::new();
    let mut next_positional = 0;
    let mut saw_named = false;

    for arg in evaluated_args {
        if let Some(name) = arg.name.as_deref() {
            saw_named = true;
            let Some(index) = params.iter().position(|param| *param == name) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            if bound_args[index].is_some() {
                return Err(EvalStatus::RuntimeFatal);
            }
            bound_args[index] = Some(arg.clone());
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        if variadic_last && next_positional >= params.len().saturating_sub(1) {
            variadic_args.push(arg.clone());
            next_positional += 1;
            continue;
        }
        if next_positional >= params.len() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if bound_args[next_positional].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        bound_args[next_positional] = Some(arg.clone());
        next_positional += 1;
    }

    if variadic_last {
        let variadic_index = params.len().saturating_sub(1);
        if let Some(named_variadic) = bound_args[variadic_index].take() {
            variadic_args.insert(0, named_variadic);
        }
    }

    Ok((bound_args, variadic_args))
}

/// Returns a required already evaluated argument by bound parameter index.
fn required_evaluated_ref_arg(
    bound_args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Result<&EvaluatedCallArg, EvalStatus> {
    bound_args
        .get(index)
        .and_then(Option::as_ref)
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns an optional already evaluated argument by bound parameter index.
fn optional_evaluated_ref_arg(
    bound_args: &[Option<EvaluatedCallArg>],
    index: usize,
) -> Option<&EvaluatedCallArg> {
    bound_args.get(index).and_then(Option::as_ref)
}
