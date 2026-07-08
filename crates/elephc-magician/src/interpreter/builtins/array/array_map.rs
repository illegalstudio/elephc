//! Purpose:
//! Declarative eval registry entry for `array_map`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::super::*;

eval_builtin! {
    name: "array_map",
    area: Array,
    params: [callback, array],
    variadic: arrays,
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_map` array builtin.
pub(in crate::interpreter) fn eval_array_map_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_map(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_map` array builtin.
pub(in crate::interpreter) fn eval_array_map_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, arrays)) = evaluated_args.split_first() else { return Err(EvalStatus::RuntimeFatal); };
    eval_array_map_result(*callback, arrays, context, values)
}
