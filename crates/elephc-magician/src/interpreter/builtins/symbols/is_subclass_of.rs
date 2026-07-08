//! Purpose:
//! Eval registry entry for `is_subclass_of`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Relationship semantics are shared with `is_a()` because PHP only changes
//!   the self-match and default string-allowance rules.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_subclass_of",
    area: Symbols,
    params: [object_or_class, r#class, allow_string = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `is_subclass_of(...)` calls through the `is_a` relation owner.
pub(in crate::interpreter) fn eval_is_subclass_of_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::is_a::eval_builtin_is_a_relation("is_subclass_of", args, context, scope, values)
}

/// Evaluates materialized `is_subclass_of(...)` arguments through the `is_a` relation owner.
pub(in crate::interpreter) fn eval_is_subclass_of_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::is_a::eval_is_a_relation_result("is_subclass_of", evaluated_args, context, values)
}
