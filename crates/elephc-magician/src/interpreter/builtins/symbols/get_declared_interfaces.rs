//! Purpose:
//! Declarative eval registry entry for `get_declared_interfaces`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the declared-symbols helper.

eval_builtin! {
    name: "get_declared_interfaces",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `get_declared_interfaces` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_declared_interfaces_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_names::eval_builtin_get_declared_symbols("get_declared_interfaces", args, context, values)
}

/// Dispatches evaluated-argument calls for the `get_declared_interfaces` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_declared_interfaces_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() { super::class_names::eval_get_declared_symbols_result("get_declared_interfaces", context, values) } else { Err(EvalStatus::RuntimeFatal) }
}
