//! Purpose:
//! Declarative eval registry entry for `array_unique`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-unique hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_unique",
    area: Array,
    direct: ArrayUnique,
    values: ArrayUnique,
}
/// Dispatches direct eval calls for the `array_unique` array builtin.
pub(in crate::interpreter) fn eval_array_unique_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_unique(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_unique` array builtin.
pub(in crate::interpreter) fn eval_array_unique_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_array_unique_result(*array, values),
        [array, flags] => {
            let flags = eval_int_value(*flags, values)?;
            eval_array_unique_with_flags(*array, flags, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_unique()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_unique(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [array] => {
            let array = eval_expr(array, context, scope, values)?;
            eval_array_unique_result(array, values)
        }
        [array, flags] => {
            let array = eval_expr(array, context, scope, values)?;
            let flags = eval_expr(flags, context, scope, values)?;
            let flags = eval_int_value(flags, values)?;
            eval_array_unique_with_flags(array, flags, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Builds `array_unique($array, $flags)`: the first occurrence of each value wins.
///
/// `SORT_REGULAR` (0) compares with `==`, `SORT_NUMERIC` (1) as numbers, and every other flag
/// (`SORT_STRING` 2, `SORT_LOCALE_STRING` 5) as strings, like the one-argument form.
/// Laravel's `Collection::unique()` calls `array_unique($items, SORT_REGULAR)`.
pub(in crate::interpreter) fn eval_array_unique_with_flags(
    array: RuntimeCellHandle,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if flags != 0 && flags != 1 {
        return eval_array_unique_result(array, values);
    }
    let len = values.array_len(array)?;
    let mut kept = Vec::<RuntimeCellHandle>::with_capacity(len);
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let mut duplicate = false;
        for seen in &kept {
            let equal = if flags == 0 {
                let same = eval_values_loosely_equal(*seen, value, values)?;
                values.bool_value(same)?
            } else {
                let left = values.cast_float(*seen)?;
                let right = values.cast_float(value)?;
                values.compare(EvalBinOp::LooseEq, left, right)?
            };
            if values.truthy(equal)? {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            continue;
        }
        kept.push(value);
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds `array_unique()` by comparing values with PHP's default string comparison mode.
pub(in crate::interpreter) fn eval_array_unique_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut seen = Vec::<Vec<u8>>::with_capacity(len);
    let mut result = values.assoc_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let unique_key = values.string_bytes(value)?;
        if seen.iter().any(|seen_key| seen_key == &unique_key) {
            continue;
        }
        seen.push(unique_key);
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

