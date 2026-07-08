//! Purpose:
//! Declarative eval registry entry for `array_diff_key`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_diff_key",
    area: Array,
    params: [array],
    variadic: arrays,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_diff_key` array builtin.
pub(in crate::interpreter) fn eval_array_diff_key_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_key_set("array_diff_key", args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_diff_key` array builtin.
pub(in crate::interpreter) fn eval_array_diff_key_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_key_set_result("array_diff_key", *left, *right, values)
}
