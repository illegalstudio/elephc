//! Purpose:
//! Declarative eval registry entry for `property_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the OOP member-existence helper.

eval_builtin! {
    name: "property_exists",
    area: Symbols,
    params: [object_or_class, property],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `property_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_property_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("property_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `property_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_property_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("property_exists", evaluated_args, context, values)
}
