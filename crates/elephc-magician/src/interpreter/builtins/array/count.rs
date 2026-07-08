//! Purpose:
//! Declarative eval registry entry for `count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing count hook.

use super::super::spec::EvalBuiltinDefaultValue;

use super::super::super::*;

eval_builtin! {
    name: "count",
    area: Array,
    params: [value, mode = EvalBuiltinDefaultValue::Int(0)],
    direct: Count,
    values: Count,
}
/// Dispatches direct eval calls for the `count` array builtin.
pub(in crate::interpreter) fn eval_count_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_count(args, context, scope, values)
}

/// Dispatches evaluated-argument eval calls for the `count` array builtin.
pub(in crate::interpreter) fn eval_count_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [value] => eval_count_result(*value, None, context, values),
        [value, mode] => eval_count_result(*value, Some(*mode), context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
