//! Purpose:
//! Core non-mutating array builtins such as aggregate, fill, map, reduce, and walk.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays` re-exports.
//!
//! Key details:
//! - Array cells remain opaque runtime handles and are manipulated through
//!   `RuntimeValueOps`.

use super::super::super::*;
use super::super::*;

pub(in crate::interpreter) fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.abs(value)
}

/// Evaluates PHP array aggregate builtins over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_aggregate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_aggregate_result(name, array, values)
}

/// Computes `array_sum()` or `array_product()` through eval's numeric value hooks.
pub(in crate::interpreter) fn eval_array_aggregate_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = match name {
        "array_sum" => values.int(0)?,
        "array_product" => values.int(1)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = match name {
            "array_sum" => values.add(result, value)?,
            "array_product" => values.mul(result, value)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
    }
    Ok(result)
}

/// Evaluates PHP `array_combine()` over key and value array expressions.
pub(in crate::interpreter) fn eval_builtin_array_combine(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let values_array = eval_expr(values_array, context, scope, values)?;
    eval_array_combine_result(keys, values_array, values)
}

/// Builds the associative result for `array_combine()` from two eval arrays.
pub(in crate::interpreter) fn eval_array_combine_result(
    keys: RuntimeCellHandle,
    values_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    if len != values.array_len(values_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }

    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        let target_key = values.cast_string(target_key)?;
        let value_key = values.array_iter_key(values_array, position)?;
        let value = values.array_get(values_array, value_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_column()` over row-array and column-key expressions.
pub(in crate::interpreter) fn eval_builtin_array_column(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, column_key] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let column_key = eval_expr(column_key, context, scope, values)?;
    eval_array_column_result(array, column_key, values)
}

/// Builds `array_column()` by extracting present row columns into a reindexed array.
pub(in crate::interpreter) fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    let mut output_index = 0_i64;
    for position in 0..len {
        let row_key = values.array_iter_key(array, position)?;
        let row = values.array_get(array, row_key)?;
        if !matches!(values.type_tag(row)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            continue;
        }
        let exists = values.array_key_exists(column_key, row)?;
        if !values.truthy(exists)? {
            continue;
        }
        let column = values.array_get(row, column_key)?;
        let target_key = values.int(output_index)?;
        output_index = output_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        result = values.array_set(result, target_key, column)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill()` over start, count, and value expressions.
pub(in crate::interpreter) fn eval_builtin_array_fill(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, count, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let start = eval_expr(start, context, scope, values)?;
    let count = eval_expr(count, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_result(start, count, value, values)
}

/// Builds an `array_fill()` result with PHP's explicit integer key range.
pub(in crate::interpreter) fn eval_array_fill_result(
    start: RuntimeCellHandle,
    count: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let start = eval_int_value(start, values)?;
    let count = eval_int_value(count, values)?;
    if count < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let count = usize::try_from(count).map_err(|_| EvalStatus::RuntimeFatal)?;
    let mut result = if start == 0 {
        values.array_new(count)?
    } else {
        values.assoc_new(count)?
    };
    for offset in 0..count {
        let offset = i64::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        let key = start.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?;
        let key = values.int(key)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_fill_keys()` over key-array and value expressions.
pub(in crate::interpreter) fn eval_builtin_array_fill_keys(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let keys = eval_expr(keys, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    eval_array_fill_keys_result(keys, value, values)
}

/// Builds an `array_fill_keys()` result preserving the source key iteration order.
pub(in crate::interpreter) fn eval_array_fill_keys_result(
    keys: RuntimeCellHandle,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(keys)?;
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(keys, position)?;
        let target_key = values.array_get(keys, source_key)?;
        result = values.array_set(result, target_key, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_map()` for one source array and a callable or null callback.
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
                array_target = Some(eval_array_mutation_lvalue_arg(arg, context, scope, values)?);
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

/// Evaluates PHP `array_walk()` for by-value callable dispatch.
pub(in crate::interpreter) fn eval_builtin_array_walk(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, callback] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    let callback = eval_expr(callback, context, scope, values)?;
    eval_array_walk_result_from_scope(array, callback, Some(scope), context, values)
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
