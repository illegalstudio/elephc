//! Purpose:
//! Declarative eval registry entry for `array_chunk`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_chunk",
    area: Array,
    params: [array, length],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_chunk` array builtin.
pub(in crate::interpreter) fn eval_array_chunk_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_chunk(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_chunk` array builtin.
pub(in crate::interpreter) fn eval_array_chunk_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array, length] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_chunk_result(*array, *length, values)
}
