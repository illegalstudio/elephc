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
