//! Purpose:
//! Declarative eval registry entry for `get_class_methods`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the OOP introspection helper.

eval_builtin! {
    name: "get_class_methods",
    area: Symbols,
    params: [object_or_class],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `get_class_methods` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_methods_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("get_class_methods", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `get_class_methods` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_class_methods_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("get_class_methods", evaluated_args, context, values)
}
