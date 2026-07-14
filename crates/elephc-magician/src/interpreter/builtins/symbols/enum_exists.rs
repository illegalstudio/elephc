//! Purpose:
//! Eval registry entry for `enum_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Class-like existence semantics are shared with `trait_exists()`.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "enum_exists",
    area: Symbols,
    params: [r#enum, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `enum_exists(...)` calls through the `trait_exists` owner.
pub(in crate::interpreter) fn eval_enum_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trait_exists::eval_builtin_class_like_exists("enum_exists", args, context, scope, values)
}

/// Evaluates materialized `enum_exists(...)` arguments through the `trait_exists` owner.
pub(in crate::interpreter) fn eval_enum_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trait_exists::eval_class_like_exists_result("enum_exists", evaluated_args, context, values)
}
