//! Purpose:
//! Declarative eval registry entry for `array_key_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the runtime key-existence hook.

use super::super::super::*;

eval_builtin! {
    name: "array_key_exists",
    area: Array,
    params: [key, array],
    direct: ArrayKeyExists,
    values: ArrayKeyExists,
}
/// Dispatches direct eval calls for the `array_key_exists` array builtin.
pub(in crate::interpreter) fn eval_array_key_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_key_exists(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_key_exists` array builtin.
pub(in crate::interpreter) fn eval_array_key_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [key, array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    values.array_key_exists(*key, *array)
}
