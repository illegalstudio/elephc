//! Purpose:
//! Declarative eval registry entry for `trait_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-like existence probe.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "trait_exists",
    area: Symbols,
    params: [r#trait, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `trait_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_trait_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_names::eval_builtin_class_like_exists("trait_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `trait_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_trait_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::class_names::eval_class_like_exists_result("trait_exists", evaluated_args, context, values)
}
