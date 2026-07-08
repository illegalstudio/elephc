//! Purpose:
//! Declarative eval registry entry for `is_callable`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct and dynamic-ref paths preserve `$callable_name` writeback elsewhere.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_callable",
    area: Symbols,
    params: [
        value,
        syntax_only = EvalBuiltinDefaultValue::Bool(false),
        callable_name: by_ref = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [callable_name],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `is_callable(...)` calls inside an eval fragment.
pub(in crate::interpreter) fn eval_is_callable_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::callable_probe::eval_builtin_is_callable(args, context, scope, values)
}

/// Evaluates materialized `is_callable(...)` arguments.
pub(in crate::interpreter) fn eval_is_callable_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::callable_probe::eval_is_callable_with_values(evaluated_args, context, values)
}
