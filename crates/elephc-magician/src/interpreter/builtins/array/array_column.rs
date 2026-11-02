//! Purpose:
//! Declarative eval registry entry for `array_column`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    contract: "array_column",
    area: Array,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_column` array builtin.
pub(in crate::interpreter) fn eval_array_column_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_column(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_column` array builtin.
pub(in crate::interpreter) fn eval_array_column_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, column_key] => eval_array_column_result(*array, *column_key, None, values),
        [array, column_key, index_key] => {
            eval_array_column_result(*array, *column_key, Some(*index_key), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `array_column()` over row-array and column-key expressions.
pub(in crate::interpreter) fn eval_builtin_array_column(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, column_key, index_key) = match args {
        [array, column_key] => (array, column_key, None),
        [array, column_key, index_key] => (array, column_key, Some(index_key)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let array = eval_expr(array, context, scope, values)?;
    let column_key = eval_expr(column_key, context, scope, values)?;
    let index_key = match index_key {
        Some(expr) => Some(eval_expr(expr, context, scope, values)?),
        None => None,
    };
    eval_array_column_result(array, column_key, index_key, values)
}

/// Builds `array_column()`: the `column_key` entry of each row (the whole row for a null key),
/// keyed by the row's `index_key` entry when that is given and present, else appended.
pub(in crate::interpreter) fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    index_key: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    let whole_row = values.is_null(column_key)?;
    let index_key = match index_key {
        Some(key) if !values.is_null(key)? => Some(key),
        _ => None,
    };
    for position in 0..len {
        let row_key = values.array_iter_key(array, position)?;
        let row = values.array_get(array, row_key)?;
        if !matches!(values.type_tag(row)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
            continue;
        }
        let column = if whole_row {
            row
        } else {
            let exists = values.array_key_exists(column_key, row)?;
            if !values.truthy(exists)? {
                continue;
            }
            values.array_get(row, column_key)?
        };
        let mut target_key = None;
        if let Some(index_key) = index_key {
            let has_index = values.array_key_exists(index_key, row)?;
            if values.truthy(has_index)? {
                let candidate = values.array_get(row, index_key)?;
                if matches!(
                    values.type_tag(candidate)?,
                    EVAL_TAG_INT | EVAL_TAG_STRING
                ) {
                    target_key = Some(candidate);
                }
            }
        }
        let target_key = match target_key {
            Some(key) => key,
            None => {
                let next = values.array_next_index(result)?.unwrap_or(0);
                values.int(next)?
            }
        };
        result = values.array_set(result, target_key, column)?;
    }
    Ok(result)
}
