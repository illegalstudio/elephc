//! Purpose:
//! Declarative eval registry entry for `is_subclass_of`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-relation predicate helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_subclass_of",
    area: Symbols,
    params: [object_or_class, r#class, allow_string = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `is_subclass_of` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_subclass_of_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_relations::eval_builtin_is_a_relation("is_subclass_of", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `is_subclass_of` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_subclass_of_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_relations::eval_is_a_relation_result("is_subclass_of", evaluated_args, context, values)
}
