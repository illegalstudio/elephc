//! Purpose:
//! Declarative eval registry entry for `array_reduce`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_reduce",
    area: Array,
    params: [array, callback, initial = EvalBuiltinDefaultValue::Null],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_reduce(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, callback] => {
            let initial = values.null()?;
            eval_array_reduce_result(*array, *callback, initial, context, values)
        }
        [array, callback, initial] => eval_array_reduce_result(*array, *callback, *initial, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_reduce()` with an optional initial carry value.
pub(in crate::interpreter) fn eval_builtin_array_reduce(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, callback, initial) = match args {
        [array, callback] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            (array, callback, values.null()?)
        }
        [array, callback, initial] => {
            let array = eval_expr(array, context, scope, values)?;
            let callback = eval_expr(callback, context, scope, values)?;
            let initial = eval_expr(initial, context, scope, values)?;
            (array, callback, initial)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_array_reduce_result_from_scope(array, callback, initial, Some(scope), context, values)
}

/// Reduces one eval array by invoking a callable with carry and item cells.
pub(in crate::interpreter) fn eval_array_reduce_result(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    initial: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_reduce_result_from_scope(array, callback, initial, None, context, values)
}

/// Reduces one eval array with optional lexical scope for callback names.
fn eval_array_reduce_result_from_scope(
    array: RuntimeCellHandle,
    callback: RuntimeCellHandle,
    initial: RuntimeCellHandle,
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_callable_with_optional_scope(callback, context, lexical_scope, values)?;
    let len = values.array_len(array)?;
    let mut carry = initial;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        carry =
            eval_evaluated_callable_with_values(&callback, vec![carry, value], context, values)?;
    }
    Ok(carry)
}
