//! Purpose:
//! Declarative eval registry entry for `array_fill_keys`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_fill_keys",
    area: Array,
    params: [keys, value],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_fill_keys` array builtin.
pub(in crate::interpreter) fn eval_array_fill_keys_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_fill_keys(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_fill_keys` array builtin.
pub(in crate::interpreter) fn eval_array_fill_keys_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [keys, value] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_fill_keys_result(*keys, *value, values)
}
