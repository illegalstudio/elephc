//! Purpose:
//! Declarative eval registry entry for `array_walk_recursive`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls bind the array by reference and hand the callback each LEAF element as a
//!   nested reference target, so a `function (&$value)` callback writes through to the caller's
//!   array at any depth. Nested arrays are descended into, never passed to the callback -- PHP's
//!   rule.
//! - The optional third argument is forwarded as the callback's third parameter, as PHP does.

use super::super::super::*;

eval_builtin! {
    contract: "array_walk_recursive",
    area: Array,
    direct: none,
    values: ArrayMutating,
}

/// Dispatches by-value callable eval calls (`call_user_func('array_walk_recursive', ...)`).
pub(in crate::interpreter) fn eval_array_walk_recursive_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, callback, extra) = match evaluated_args {
        [array, callback] => (*array, *callback, None),
        [array, callback, extra] => (*array, *callback, Some(*extra)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.warning(
        "Warning: array_walk_recursive(): Argument #1 ($array) must be passed by reference, value given\n",
    )?;
    let callback = eval_callable_with_optional_scope(callback, context, None, values)?;
    callback.with_values(context, values, |callback, context, values| {
        eval_array_walk_recursive_values(array, callback, extra, context, values)?;
        values.bool_value(true)
    })
}

/// Evaluates direct PHP `array_walk_recursive()` calls, preserving leaf by-ref targets.
pub(in crate::interpreter) fn eval_builtin_array_walk_recursive_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut array_target = None;
    let mut callback = None;
    let mut extra = None;
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
                2 => "arg",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };
        match parameter {
            "array" if array_target.is_none() => {
                array_target = Some(super::mutation::eval_array_mutation_lvalue_arg(
                    arg, context, scope, values,
                )?);
            }
            "callback" if callback.is_none() => {
                callback = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "arg" if extra.is_none() => {
                extra = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    let (_, array_target) = array_target.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = eval_callable_with_optional_scope(callback, context, Some(scope), values)?;
    callback.with_values(context, values, |callback, context, values| {
        eval_array_walk_recursive_ref(&array_target, callback, extra, context, values)?;
        values.bool_value(true)
    })
}

/// Returns whether a runtime value is an array PHP's recursive walk descends into.
fn is_walkable_array(value: RuntimeCellHandle, values: &mut impl RuntimeValueOps) -> Result<bool, EvalStatus> {
    Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC))
}

/// Walks one level through its reference target, re-reading the array each step so a callback
/// that grows or replaces it is seen exactly as PHP would see it.
fn eval_array_walk_recursive_ref(
    array_target: &EvalReferenceTarget,
    callback: &EvaluatedCallable,
    extra: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = {
        let current = eval_reference_target_value(array_target, context, values)?;
        values.array_len(current)?
    };
    for position in 0..len {
        let current = eval_reference_target_value(array_target, context, values)?;
        if position >= values.array_len(current)? {
            break;
        }
        let key = values.array_iter_key(current, position)?;
        let value = values.array_get(current, key)?;
        let reference_key =
            eval_array_reference_key(key, values)?.ok_or(EvalStatus::RuntimeFatal)?;
        let child = EvalReferenceTarget::NestedArrayElement {
            array_target: Box::new(array_target.clone()),
            index: reference_key,
        };
        if is_walkable_array(value, values)? {
            eval_array_walk_recursive_ref(&child, callback, extra, context, values)?;
            continue;
        }
        let mut args = vec![
            EvaluatedCallArg {
                name: None,
                value: value.borrowed(),
                ref_target: Some(child),
            },
            EvaluatedCallArg {
                name: None,
                value: key.borrowed(),
                ref_target: None,
            },
        ];
        if let Some(extra) = extra {
            args.push(EvaluatedCallArg {
                name: None,
                value: extra.borrowed(),
                ref_target: None,
            });
        }
        let _ = eval_evaluated_callable_with_call_array_args(callback, args, context, values)?;
    }
    Ok(())
}

/// Walks a by-value array: the callback sees each leaf value and key, and nothing is written back.
fn eval_array_walk_recursive_values(
    array: RuntimeCellHandle,
    callback: &EvaluatedCallable,
    extra: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        if is_walkable_array(value, values)? {
            eval_array_walk_recursive_values(value, callback, extra, context, values)?;
            continue;
        }
        let mut args = vec![value, key];
        if let Some(extra) = extra {
            args.push(extra);
        }
        let _ = eval_evaluated_callable_with_values(callback, args, context, values)?;
    }
    Ok(())
}
