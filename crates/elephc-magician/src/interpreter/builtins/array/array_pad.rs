//! Purpose:
//! Declarative eval registry entry for `array_pad`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-pad hook.

use super::super::super::*;

eval_builtin! {
    name: "array_pad",
    area: Array,
    params: [array, length, value],
    direct: ArrayPad,
    values: ArrayPad,
}
/// Dispatches direct eval calls for the `array_pad` array builtin.
pub(in crate::interpreter) fn eval_array_pad_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_pad(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_pad` array builtin.
pub(in crate::interpreter) fn eval_array_pad_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length, value] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_pad_result(*array, *length, *value, values)
}
