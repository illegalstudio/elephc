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
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, column_key] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_column_result(*array, *column_key, context, values)
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
    eval_array_column_result(array, column_key, context, values)
}

/// Builds `array_column()` by extracting present row columns into a reindexed array.
pub(in crate::interpreter) fn eval_array_column_result(
    array: RuntimeCellHandle,
    column_key: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    checked_array_keys::require_key(column_key, true, context, values)?;
    let whole_row = values.is_null(column_key)?;
    let mut result = super::super::collection_builder::EvalArrayBuilder::indexed(values, len)?;
    let mut output_index = 0;
    for position in 0..len {
        let row = super::key_inputs::value_at(array, position, result.values())?;
        let selected = (|| {
            if !matches!(result.values().type_tag(row)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) { return Ok(None); }
            if whole_row { return result.values().retain(row).map(Some); }
            let exists = result.values().array_key_exists(column_key, row)?;
            let present = result.values().truthy(exists);
            let released = result.values().release(exists);
            let present = present?; released?;
            if !present { return Ok(None); }
            eval_array_get_result(row, column_key, context, result.values())
                .and_then(|column| promote_borrowed_result(column, result.values())).map(Some)
        })();
        let released = eval_release_value(context, result.values(), row);
        let column = match (selected, released) {
            (Ok(column), Ok(())) => column,
            (Err(status), _) => return Err(status),
            (Ok(column), Err(status)) => {
                if let Some(column) = column { let _ = eval_release_value(context, result.values(), column); }
                return Err(status);
            }
        };
        if let Some(column) = column {
            result.index(output_index, |_| Ok(column))?;
            output_index += 1;
        }
    }
    Ok(result.finish())
}
