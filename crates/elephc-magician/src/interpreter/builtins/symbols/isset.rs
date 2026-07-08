//! Purpose:
//! Declarative eval registry entry for `isset`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Direct calls stay source-sensitive so operands are checked without normal reads.

eval_builtin! {
    name: "isset",
    area: Symbols,
    params: [var],
    variadic: vars,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `isset` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_isset_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("isset", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `isset` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_isset_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("isset", evaluated_args, context, values)
}
