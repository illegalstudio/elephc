//! Purpose:
//! Declarative eval registry entry for `array_combine`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_combine",
    area: Array,
    params: [keys, values],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_combine` array builtin.
pub(in crate::interpreter) fn eval_array_combine_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_combine(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_combine` array builtin.
pub(in crate::interpreter) fn eval_array_combine_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, values_array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_combine_result(*keys, *values_array, values)
}
