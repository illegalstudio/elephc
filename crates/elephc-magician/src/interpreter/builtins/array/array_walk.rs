//! Purpose:
//! Declarative eval registry entry for `array_walk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

eval_builtin! {
    name: "array_walk",
    area: Array,
    params: [array: by_ref, callback],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_walk` array mutator.
pub(in crate::interpreter) fn eval_array_walk_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    values.warning("array_walk(): Argument #1 ($array) must be passed by reference, value given")?;
    eval_array_walk_result(*array, *callback, context, values)
}

/// Evaluates direct PHP `array_walk()` calls and preserves element by-ref targets.
pub(in crate::interpreter) fn eval_builtin_array_walk_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, array_target, callback) =
        eval_array_walk_direct_args(args, context, scope, values)?;
    eval_array_walk_ref_result_from_scope(array, array_target, callback, Some(scope), context, values)
}

/// Evaluates and binds direct `array_walk()` arguments in PHP source order.
fn eval_array_walk_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    let mut array_target = None;
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
                if array_target.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                array_target = Some(super::mutation::eval_array_mutation_lvalue_arg(arg, context, scope, values)?);
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

    let (array, array_target) = array_target.ok_or(EvalStatus::RuntimeFatal)?;
    let callback = callback.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, array_target, callback))
}

/// Walks one writable eval array by invoking a callable with element ref targets.
pub(in crate::interpreter) fn eval_array_walk_ref_result(
    array: RuntimeCellHandle,
    array_target: EvalReferenceTarget,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_walk_ref_result_from_scope(array, array_target, callback, None, context, values)
}

/// Walks one writable eval array with optional lexical scope for callback names.
fn eval_array_walk_ref_result_from_scope(
    array: RuntimeCellHandle,
    array_target: EvalReferenceTarget,
    callback: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    let len = values.array_len(array)?;
    for position in 0..len {
        let current_array = eval_reference_target_value(&array_target, context, values)?;
        let key = values.array_iter_key(current_array, position)?;
        let value = values.array_get(current_array, key)?;
        let ref_target = EvalReferenceTarget::NestedArrayElement {
            array_target: Box::new(array_target.clone()),
            index: key,
        };
        let args = vec![
            EvaluatedCallArg {
                name: None,
                value,
                ref_target: Some(ref_target),
            },
            EvaluatedCallArg {
                name: None,
                value: key,
                ref_target: None,
            },
        ];
        let _ = eval_evaluated_callable_with_call_array_args(&callback, args, context, values)?;
    }
    values.bool_value(true)
}

/// Walks one eval array by invoking a callable with value and key cells.
pub(in crate::interpreter) fn eval_array_walk_result(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_walk_result_from_scope(array, callback, None, context, values)
}

/// Walks one eval array with optional lexical scope for callback names.
fn eval_array_walk_result_from_scope(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let _ = eval_evaluated_callable_with_values(&callback, vec![value, key], context, values)?;
    }
    values.bool_value(true)
}
