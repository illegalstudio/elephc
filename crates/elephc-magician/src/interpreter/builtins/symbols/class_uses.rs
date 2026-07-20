//! Purpose:
//! Declarative eval registry entry for `class_uses`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Shared class-relation logic lives in `class_implements`.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "class_uses",
    area: Symbols,
    params: [object_or_class, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `class_uses` symbol builtin.
pub(in crate::interpreter) fn eval_class_uses_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_implements::eval_builtin_class_relation("class_uses", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `class_uses` symbol builtin.
pub(in crate::interpreter) fn eval_class_uses_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_implements::eval_class_relation_result("class_uses", evaluated_args, context, values)
}
