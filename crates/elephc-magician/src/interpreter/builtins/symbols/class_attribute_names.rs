//! Purpose:
//! Declarative eval registry entry for `class_attribute_names`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-attribute metadata helper.

eval_builtin! {
    name: "class_attribute_names",
    area: Symbols,
    params: [class_name],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `class_attribute_names` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_class_attribute_metadata("class_attribute_names", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_attribute_names` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_class_attribute_names_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_class_attribute_metadata_result("class_attribute_names", evaluated_args, context, values)
}
