//! Purpose:
//! Declarative eval registry entry for `array_reverse`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-reverse hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_reverse",
    area: Array,
    params: [array, preserve_keys = EvalBuiltinDefaultValue::Bool(false)],
    direct: ArrayReverse,
    values: ArrayReverse,
}
/// Dispatches direct eval calls for the `array_reverse` array builtin.
pub(in crate::interpreter) fn eval_array_reverse_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_reverse(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_reverse` array builtin.
pub(in crate::interpreter) fn eval_array_reverse_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_array_reverse_result(*array, false, values),
        [array, preserve_keys] => {
            let preserve_keys = values.truthy(*preserve_keys)?;
            eval_array_reverse_result(*array, preserve_keys, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
