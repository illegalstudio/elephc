//! Purpose:
//! Declarative eval registry entry for `array_sum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-aggregate hook.

use super::super::super::*;

eval_builtin! {
    name: "array_sum",
    area: Array,
    params: [array],
    direct: ArrayAggregate,
    values: ArrayAggregate,
}
/// Dispatches direct eval calls for the `array_sum` array builtin.
pub(in crate::interpreter) fn eval_array_sum_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_aggregate("array_sum", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_sum` array builtin.
pub(in crate::interpreter) fn eval_array_sum_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_aggregate_result("array_sum", *array, values)
}
