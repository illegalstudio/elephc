//! Purpose:
//! Declarative eval registry entry for `array_map`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "array_map",
    area: Array,
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
    lexical_scope: Option<&ElephcEvalScope>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, arrays)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_map_result_from_scope(*callback, arrays, lexical_scope, context, values)
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
    with_optional_evaluated_callable(callback, context, values, |callback, context, values| {
        let len = values.array_len(*array)?;
        let mut result = EvalArrayBuilder::assoc(values, len)?;
        for position in 0..len {
            result.update(|values, owners, _| {
                let key = owners.stage(values.array_iter_key(*array, position)?);
                let value = owners.stage(values.array_get(*array, key)?);
                let mapped = if let Some(callback) = callback.as_ref() {
                    owners.stage(eval_evaluated_callable_with_values(
                        callback, vec![value.borrowed()], context, values,
                    )?)
                } else {
                    value
                };
                Ok(Some((key, mapped)))
            })?;
        }
        Ok(result.finish())
    })
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
    with_optional_evaluated_callable(callback, context, values, |callback, context, values| {
        let mut lengths = Vec::with_capacity(arrays.len());
        let mut max_len = 0;
        for array in arrays {
            let len = values.array_len(*array)?;
            max_len = max_len.max(len);
            lengths.push(len);
        }

        let mut result = EvalArrayBuilder::indexed(values, max_len)?;
        for position in 0..max_len {
            result.update(|values, owners, _| {
                let mut callback_args = Vec::with_capacity(arrays.len());
                for (array, len) in arrays.iter().zip(lengths.iter()) {
                    let value = if position < *len {
                        let key = owners.stage(values.array_iter_key(*array, position)?);
                        values.array_get(*array, key)?
                    } else {
                        values.null()?
                    };
                    callback_args.push(owners.stage(value).borrowed());
                }
                let mapped = if let Some(callback) = callback.as_ref() {
                    eval_evaluated_callable_with_values(callback, callback_args, context, values)?
                } else {
                    eval_array_map_zipped_row(callback_args, values)?
                };
                let mapped = owners.stage(mapped);
                let index = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
                let key = owners.stage(values.int(index)?);
                Ok(Some((key, mapped)))
            })?;
        }
        Ok(result.finish())
    })
}

/// Builds one row for `array_map(null, $a, $b, ...)`.
pub(in crate::interpreter) fn eval_array_map_zipped_row(
    values_row: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut row = EvalArrayBuilder::indexed(values, values_row.len())?;
    for (index, value) in values_row.into_iter().enumerate() {
        row.index(index, |_| Ok(value.borrowed()))?;
    }
    Ok(row.finish())
}
