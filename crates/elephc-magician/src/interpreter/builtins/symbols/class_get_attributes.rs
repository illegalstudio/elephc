//! Purpose:
//! Declarative eval registry entry for `class_get_attributes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-attribute metadata helper.

eval_builtin! {
    name: "class_get_attributes",
    area: Symbols,
    params: [class_name],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `class_get_attributes` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_get_attributes_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("class_get_attributes", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_get_attributes` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_get_attributes_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("class_get_attributes", evaluated_args, context, values)
}
