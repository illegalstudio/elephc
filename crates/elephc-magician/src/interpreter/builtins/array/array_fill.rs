//! Purpose:
//! Declarative eval registry entry for `array_fill`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_fill",
    area: Array,
    params: [start_index, count, value],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_fill` array builtin.
pub(in crate::interpreter) fn eval_array_fill_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_fill(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_fill` array builtin.
pub(in crate::interpreter) fn eval_array_fill_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [start, count, value] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_fill_result(*start, *count, *value, values)
}
