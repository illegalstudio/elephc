//! Purpose:
//! Declarative eval registry entry for `array_reduce`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the non-mutating array hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "array_reduce",
    area: Array,
    params: [array, callback, initial = EvalBuiltinDefaultValue::Null],
    direct: Array,
    values: Array,
}
/// Dispatches direct eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_array_reduce(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `array_reduce` array builtin.
pub(in crate::interpreter) fn eval_array_reduce_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [array, callback] => {
            let initial = values.null()?;
            eval_array_reduce_result(*array, *callback, initial, context, values)
        }
        [array, callback, initial] => eval_array_reduce_result(*array, *callback, *initial, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
