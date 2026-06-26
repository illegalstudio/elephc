//! Purpose:
//! Binds direct by-reference array sort calls before delegating to sort engines.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::sort` re-exports.
//!
//! Key details:
//! - Direct calls extract a writable lvalue cell and write back the sorted
//!   replacement while preserving source-order evaluation of callback arguments.

use super::super::super::super::*;
use super::super::{eval_array_mutation_lvalue_arg, eval_write_direct_ref_target};
use super::*;

/// Evaluates direct by-reference array ordering calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_sort_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, target) = eval_array_sort_direct_arg(args, context, scope, values)?;

    let replacement = eval_array_sort_replacement(name, array, values)?;
    let result = values.bool_value(true)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
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
    let (array, target, callback) = eval_user_sort_direct_args(args, context, scope, values)?;

    let replacement = eval_user_sort_replacement(name, array, callback, context, values)?;
    let result = values.bool_value(true)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(result)
}

/// Evaluates and binds direct user-sort arguments while preserving source order.
pub(in crate::interpreter) fn eval_user_sort_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
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
                array = Some(eval_array_mutation_lvalue_arg(
                    arg, context, scope, values,
                )?);
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

    let (array, target) = array.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, target, callback))
}

/// Extracts the writable array lvalue accepted by eval array ordering builtins.
pub(in crate::interpreter) fn eval_array_sort_direct_arg(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget), EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_mutation_lvalue_arg(arg, context, scope, values)
}
