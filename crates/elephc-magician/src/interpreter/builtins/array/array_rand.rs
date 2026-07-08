//! Purpose:
//! Declarative eval registry entry for `array_rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-rand hook.

use super::super::super::*;

eval_builtin! {
    name: "array_rand",
    area: Array,
    params: [array],
    direct: ArrayRand,
    values: ArrayRand,
}
/// Dispatches direct eval calls for the `array_rand` array builtin.
pub(in crate::interpreter) fn eval_array_rand_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_rand(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_rand` array builtin.
pub(in crate::interpreter) fn eval_array_rand_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_rand_result(*array, values)
}
