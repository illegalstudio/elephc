//! Purpose:
//! Declarative eval registry entry for `function_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the builtin/function probe helper.

eval_builtin! {
    name: "function_exists",
    area: Symbols,
    params: [function],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `function_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_function_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::function_probe::eval_builtin_function_probe("function_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `function_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_function_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args { [value] => super::function_probe::eval_function_probe_result("function_exists", *value, context, values), _ => Err(EvalStatus::RuntimeFatal), }
}
