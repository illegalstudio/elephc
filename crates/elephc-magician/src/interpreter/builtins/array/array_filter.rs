//! Purpose:
//! Declarative eval registry entry for `array_filter`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_filter",
    area: Array,
    params: [
        array,
        callback = EvalBuiltinDefaultValue::Null,
        mode = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_filter` array builtin.
pub(in crate::interpreter) fn eval_array_filter_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_filter(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_filter` array builtin.
pub(in crate::interpreter) fn eval_array_filter_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array] => eval_array_filter_result(*array, None, None, context, values),
        [array, callback] => eval_array_filter_result(*array, Some(*callback), None, context, values),
        [array, callback, mode] => eval_array_filter_result(*array, Some(*callback), Some(*mode), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
