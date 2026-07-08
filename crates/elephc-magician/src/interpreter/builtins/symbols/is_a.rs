//! Purpose:
//! Declarative eval registry entry for `is_a`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the class-relation predicate helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "is_a",
    area: Symbols,
    params: [object_or_class, r#class, allow_string = EvalBuiltinDefaultValue::Bool(false)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `is_a` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_a_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("is_a", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `is_a` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_is_a_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("is_a", evaluated_args, context, values)
}
