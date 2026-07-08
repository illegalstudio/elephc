//! Purpose:
//! Declarative eval registry entry for `get_class_vars`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the OOP introspection helper.

eval_builtin! {
    name: "get_class_vars",
    area: Symbols,
    params: [r#class],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `get_class_vars` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_vars_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("get_class_vars", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class_vars` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_vars_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("get_class_vars", evaluated_args, context, values)
}
