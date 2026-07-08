//! Purpose:
//! Declarative eval registry entry for `array_flip`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-flip hook.

use super::super::super::*;

eval_builtin! {
    name: "array_flip",
    area: Array,
    params: [array],
    direct: ArrayFlip,
    values: ArrayFlip,
}
/// Dispatches direct eval calls for the `array_flip` array builtin.
pub(in crate::interpreter) fn eval_array_flip_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_flip(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_flip` array builtin.
pub(in crate::interpreter) fn eval_array_flip_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_flip_result(*array, values)
}
