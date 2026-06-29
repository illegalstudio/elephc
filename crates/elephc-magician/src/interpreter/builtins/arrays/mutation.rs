//! Purpose:
//! By-reference settype and array mutation dispatch for eval builtin calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

/// Evaluates direct by-reference `settype()` calls and writes the converted cell back.
pub(in crate::interpreter) fn eval_builtin_settype_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, target, type_name) = eval_settype_direct_args(args, context, scope, values)?;
    let Some(converted) = eval_settype_cast_value(value, type_name, values)? else {
        return values.bool_value(false);
    };
    eval_write_direct_ref_target(
        &target,
        converted,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )?;
    values.bool_value(true)
}

/// Evaluates and binds direct `settype()` arguments while preserving source order.
pub(in crate::interpreter) fn eval_settype_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    let mut var_target = None;
    let mut type_name = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "var",
                1 => "type",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "var" => {
                if var_target.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let (value, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
                let target = target.ok_or(EvalStatus::RuntimeFatal)?;
                var_target = Some((value, target));
            }
            "type" => {
                if type_name.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                type_name = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (value, target) = var_target.ok_or(EvalStatus::RuntimeFatal)?;
    let type_name = type_name.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((value, target, type_name))
}

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

/// Applies the eval-supported `settype()` scalar target conversion.
pub(in crate::interpreter) fn eval_settype_cast_value(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let type_name = values.string_bytes(type_name)?;
    let type_name = String::from_utf8_lossy(&type_name).to_ascii_lowercase();
    let converted = match type_name.as_str() {
        "bool" | "boolean" => Some(values.cast_bool(value)?),
        "float" | "double" => Some(values.cast_float(value)?),
        "int" | "integer" => Some(values.cast_int(value)?),
        "string" => Some(values.cast_string(value)?),
        _ => None,
    };
    Ok(converted)
}

/// Evaluates by-value `settype()` callable dispatch without mutating the source argument.
pub(in crate::interpreter) fn eval_settype_value_result(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning("settype(): Argument #1 ($var) must be passed by reference, value given")?;
    if let Some(converted) = eval_settype_cast_value(value, type_name, values)? {
        values.release(converted)?;
        return values.bool_value(true);
    }
    values.bool_value(false)
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
