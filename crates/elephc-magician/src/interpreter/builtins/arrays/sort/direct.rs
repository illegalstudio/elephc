//! Purpose:
//! Binds direct by-reference array sort calls before delegating to sort engines.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::sort` re-exports.
//!
//! Key details:
//! - Direct calls extract a writable variable cell and write back the sorted
//!   replacement while preserving source-order evaluation of callback arguments.

use super::super::super::super::*;
use super::*;

/// Evaluates direct by-reference array ordering calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_sort_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array_name = eval_array_sort_direct_arg(args)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let replacement = eval_array_sort_replacement(name, array, values)?;
    let result = values.bool_value(true)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates direct by-reference user-comparator sort calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_user_sort_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array_name, callback) = eval_user_sort_direct_args(args, context, scope, values)?;
    let Some(entry) =
        scope_entry(context, scope, &array_name).filter(|entry| entry.flags().is_visible())
    else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = entry.cell();
    let ownership = entry.flags().ownership;
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }

    let replacement = eval_user_sort_replacement(name, array, callback, context, values)?;
    let result = values.bool_value(true)?;
    for replaced in set_scope_cell(context, scope, array_name, replacement, ownership)? {
        values.release(replaced)?;
    }
    Ok(result)
}

/// Evaluates and binds direct user-sort arguments while preserving source order.
pub(in crate::interpreter) fn eval_user_sort_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(String, RuntimeCellHandle), EvalStatus> {
    let mut array = None;
    let mut callback = None;
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
                0 => "array",
                1 => "callback",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "array" => {
                if array.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let EvalExpr::LoadVar(name) = arg.value() else {
                    return Err(EvalStatus::RuntimeFatal);
                };
                array = Some(name.clone());
            }
            "callback" => {
                if callback.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                callback = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let array = array.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, callback))
}

/// Extracts the direct variable argument accepted by eval array ordering builtins.
pub(in crate::interpreter) fn eval_array_sort_direct_arg(
    args: &[EvalCallArg],
) -> Result<String, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if arg.is_spread() || !matches!(arg.name(), None | Some("array")) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let EvalExpr::LoadVar(name) = arg.value() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    Ok(name.clone())
}
