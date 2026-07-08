//! Purpose:
//! Declarative eval registry entry for `spl_autoload_unregister`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL autoload registration stub.

eval_builtin! {
    name: "spl_autoload_unregister",
    area: Symbols,
    params: [callback],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_autoload_unregister` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_unregister_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_builtin_symbols_call_impl("spl_autoload_unregister", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_autoload_unregister` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_unregister_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::dispatch::eval_symbols_values_result_impl("spl_autoload_unregister", evaluated_args, context, values)
}
