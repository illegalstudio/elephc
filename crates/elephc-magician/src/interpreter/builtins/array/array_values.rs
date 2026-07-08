//! Purpose:
//! Eval registry entry and implementation for `array_values`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Output is always a sequential indexed array containing input values in
//!   iteration order.

use super::super::super::*;

eval_builtin! {
    name: "array_values",
    area: Array,
    params: [array],
    direct: ArrayValues,
    values: ArrayValues,
}

/// Evaluates PHP `array_values()` over one eval array expression.
pub(in crate::interpreter) fn eval_builtin_array_values(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_values_result(array, values)
}

/// Builds the sequential result array for `array_values()`.
pub(in crate::interpreter) fn eval_array_values_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}
