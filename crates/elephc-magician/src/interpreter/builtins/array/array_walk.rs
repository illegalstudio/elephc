//! Purpose:
//! Declarative eval registry entry for `array_walk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{with_collection_call, with_collection_cells};
use crate::runtime_hooks::owner_frames::StagedRuntimeOwners;

eval_builtin! {
    contract: "array_walk",
    area: Array,
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
    with_collection_call(context, values, |context, values, owners, result| {
        let (array, array_target, callback) =
            eval_array_walk_direct_args(args, context, scope, values, owners)?;
        let value = eval_array_walk_ref_result_from_scope(
            array, array_target, callback, Some(scope), context, values,
        )?;
        result.replace(Some(value));
        Ok(())
    })
}

/// Evaluates and binds direct `array_walk()` arguments in PHP source order.
fn eval_array_walk_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    owners: &StagedRuntimeOwners,
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
                array_target = Some(super::mutation::eval_array_mutation_lvalue_arg(
                    arg, context, scope, values, owners,
                )?);
            }
            "callback" => {
                if callback.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let value = eval_owned_expr(arg.value(), context, scope, values)?;
                callback = Some(owners.stage(value).borrowed());
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
    callback.with_values(context, values, |callback, context, values| {
        let len = values.array_len(array)?;
        for position in 0..len {
            with_collection_cells(values, |values, owners| {
                let current_array = owners.stage(eval_reference_target_value(&array_target, context, values)?);
                let key = owners.stage(values.array_iter_key(current_array, position)?);
                let value = owners.stage(values.array_get(current_array, key)?);
                let ref_target = EvalReferenceTarget::NestedArrayElement {
                    array_target: Box::new(array_target.clone()),
                    index: key.borrowed(),
                };
                let args = vec![
                    EvaluatedCallArg {
                        evaluation_owner: None,
                        name: None,
                        value: value.borrowed(),
                        ref_target: Some(ref_target),
                    },
                    EvaluatedCallArg {
                        evaluation_owner: None,
                        name: None,
                        value: key.borrowed(),
                        ref_target: None,
                    },
                ];
                owners.stage(eval_evaluated_callable_with_call_array_args(&callback, args, context, values)?);
                Ok(())
            })?;
        }
        values.bool_value(true)
    })
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
    callback.with_values(context, values, |callback, context, values| {
        let len = values.array_len(array)?;
        for position in 0..len {
            with_collection_cells(values, |values, owners| {
                let key = owners.stage(values.array_iter_key(array, position)?);
                let value = owners.stage(values.array_get(array, key)?);
                owners.stage(eval_evaluated_callable_with_values(&callback, vec![value.borrowed(), key.borrowed()], context, values)?);
                Ok(())
            })?;
        }
        values.bool_value(true)
    })
}
