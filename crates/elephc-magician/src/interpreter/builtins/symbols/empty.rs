//! Purpose:
//! Declarative eval registry entry for `empty`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so missing variables are not evaluated normally.

eval_builtin! {
    name: "empty",
    area: Symbols,
    params: [value],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `empty` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_empty_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("empty", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `empty` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_empty_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("empty", evaluated_args, context, values)
}
