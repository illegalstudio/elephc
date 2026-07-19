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
    name: "array_column",
    area: Array,
    params: [array, column_key],
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
    let [array, column_key] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_column_result(*array, *column_key, values)
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
