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
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;

eval_builtin! {
    contract: "array_values",
    area: Array,
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
    let mut result = EvalArrayBuilder::indexed(values, len)?;
    for position in 0..len {
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let value = owners.stage(values.array_get(array, source_key)?);
            let index = owners.stage(values.int(position as i64)?);
            Ok(Some((index, value)))
        })?;
    }
    Ok(result.finish())
}
/// Dispatches direct eval calls for the `array_values` array builtin.
pub(in crate::interpreter) fn eval_array_values_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_values(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_values` array builtin.
pub(in crate::interpreter) fn eval_array_values_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_values_result(*array, values)
}
