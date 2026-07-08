//! Purpose:
//! Declarative eval registry entry for `array_slice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-slice hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_slice",
    area: Array,
    params: [array, offset, length = EvalBuiltinDefaultValue::Null],
    direct: ArraySlice,
    values: ArraySlice,
}
/// Dispatches direct eval calls for the `array_slice` array builtin.
pub(in crate::interpreter) fn eval_array_slice_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_slice(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_slice` array builtin.
pub(in crate::interpreter) fn eval_array_slice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, offset] => eval_array_slice_result(*array, *offset, None, values),
        [array, offset, length] => eval_array_slice_result(*array, *offset, Some(*length), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
