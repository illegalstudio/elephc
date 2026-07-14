//! Purpose:
//! Declarative eval registry entry for `array_map`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_map",
    area: Array,
    params: [callback, array],
    variadic: arrays,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_map` array builtin.
pub(in crate::interpreter) fn eval_array_map_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_map(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_map` array builtin.
pub(in crate::interpreter) fn eval_array_map_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, arrays)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_map_result(*callback, arrays, context, values)
}

/// Evaluates PHP `array_map()` for one or more arrays and an optional callback.
pub(in crate::interpreter) fn eval_builtin_array_map(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, arrays)) = args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let mut evaluated_arrays = Vec::with_capacity(arrays.len());
    for array in arrays {
        evaluated_arrays.push(eval_expr(array, context, scope, values)?);
    }
    eval_array_map_result_from_scope(callback, &evaluated_arrays, Some(scope), context, values)
}

/// Maps one eval array with PHP key preservation for the one-array form.
pub(in crate::interpreter) fn eval_array_map_result(
    callback: RuntimeCellHandle,
    arrays: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_array_map_result_from_scope(callback, arrays, None, context, values)
}

/// Maps one or more eval arrays with optional lexical scope for callback names.
fn eval_array_map_result_from_scope(
    callback: RuntimeCellHandle,
    arrays: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = arrays else {
        return eval_array_map_variadic_result_from_scope(
            callback,
            arrays,
            lexical_scope,
            context,
            values,
        );
    };
    let callback = if values.is_null(callback)? {
        None
    } else {
        Some(eval_callable_with_optional_scope(
            callback,
            context,
            lexical_scope,
            values,
        )?)
    };
    let len = values.array_len(*array)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(*array, position)?;
        let value = values.array_get(*array, key)?;
        let mapped = if let Some(callback) = callback.as_ref() {
            eval_evaluated_callable_with_values(callback, vec![value], context, values)?
        } else {
            value
        };
        result = values.array_set(result, key, mapped)?;
    }
    Ok(result)
}

/// Maps multiple eval arrays with optional lexical scope for callback names.
fn eval_array_map_variadic_result_from_scope(
    callback: RuntimeCellHandle,
    arrays: &[RuntimeCellHandle],
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if arrays.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let callback = if values.is_null(callback)? {
        None
    } else {
        Some(eval_callable_with_optional_scope(
            callback,
            context,
            lexical_scope,
            values,
        )?)
    };
    let mut lengths = Vec::with_capacity(arrays.len());
    let mut max_len = 0;
    for array in arrays {
        let len = values.array_len(*array)?;
        max_len = max_len.max(len);
        lengths.push(len);
    }

    let mut result = values.array_new(max_len)?;
    for position in 0..max_len {
        let mut callback_args = Vec::with_capacity(arrays.len());
        for (array, len) in arrays.iter().zip(lengths.iter()) {
            let value = if position < *len {
                let key = values.array_iter_key(*array, position)?;
                values.array_get(*array, key)?
            } else {
                values.null()?
            };
            callback_args.push(value);
        }
        let mapped = if let Some(callback) = callback.as_ref() {
            eval_evaluated_callable_with_values(callback, callback_args, context, values)?
        } else {
            eval_array_map_zipped_row(callback_args, values)?
        };
        let key = values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, mapped)?;
    }
    Ok(result)
}

/// Builds one row for `array_map(null, $a, $b, ...)`.
pub(in crate::interpreter) fn eval_array_map_zipped_row(
    values_row: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut row = values.array_new(values_row.len())?;
    for (index, value) in values_row.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        row = values.array_set(row, key, value)?;
    }
    Ok(row)
}
