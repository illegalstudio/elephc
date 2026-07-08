//! Purpose:
//! By-reference array mutation dispatch for eval builtin calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Captures the first by-reference array mutator argument as a writable lvalue.
pub(in crate::interpreter) fn eval_array_mutation_lvalue_arg(
    arg: &EvalCallArg,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
    let target = target.ok_or(EvalStatus::RuntimeFatal)?;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok((array, target))
}

/// Evaluates direct by-reference `array_pop()` / `array_shift()` calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_pop_shift_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "settype" {
        return eval_builtin_settype_call(args, context, scope, values);
    }
    if matches!(name, "array_push" | "array_unshift") {
        return eval_builtin_array_push_unshift_call(name, args, context, scope, values);
    }
    if name == "array_splice" {
        return eval_builtin_array_splice_call(args, context, scope, values);
    }
    if name == "array_walk" {
        return eval_builtin_array_walk_call(args, context, scope, values);
    }
    if matches!(
        name,
        "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
    ) {
        return eval_builtin_array_sort_call(name, args, context, scope, values);
    }
    if matches!(name, "uasort" | "uksort" | "usort") {
        return eval_builtin_user_sort_call(name, args, context, scope, values);
    }

    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let (array, target) = eval_array_mutation_lvalue_arg(arg, context, scope, values)?;

    let (result, replacement) = eval_array_pop_shift_replacement(name, array, values)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}

/// Evaluates direct by-reference `array_push()` / `array_unshift()` calls.
pub(in crate::interpreter) fn eval_builtin_array_push_unshift_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 || !eval_call_args_are_plain_positional(args) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (array, target) = eval_array_mutation_lvalue_arg(&args[0], context, scope, values)?;
    let mut inserted = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        inserted.push(eval_expr(arg.value(), context, scope, values)?);
    }

    let replacement = eval_array_push_unshift_replacement(name, array, &inserted, values)?;
    let result = eval_array_push_unshift_count_result(array, inserted.len(), values)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}
