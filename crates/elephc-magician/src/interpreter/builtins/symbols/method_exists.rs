//! Purpose:
//! Declarative eval registry entry for `method_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the OOP member-existence helper.

eval_builtin! {
    name: "method_exists",
    area: Symbols,
    params: [object_or_class, method],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `method_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_method_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_member_exists("method_exists", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `method_exists` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_method_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_member_exists_result("method_exists", evaluated_args, context, values)
}
