//! Purpose:
//! Declarative eval registry entry for `class_get_attributes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared class-attribute metadata logic lives in `class_attribute_names`.

eval_builtin! {
    name: "class_get_attributes",
    area: Symbols,
    params: [class_name],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `class_get_attributes` symbol builtin.
pub(in crate::interpreter) fn eval_class_get_attributes_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_attribute_names::eval_builtin_class_attribute_metadata(
        "class_get_attributes",
        args,
        context,
        scope,
        values,
    )
}

/// Dispatches evaluated-argument calls for the `class_get_attributes` symbol builtin.
pub(in crate::interpreter) fn eval_class_get_attributes_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_attribute_names::eval_class_attribute_metadata_result(
        "class_get_attributes",
        evaluated_args,
        context,
        values,
    )
}
