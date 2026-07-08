//! Purpose:
//! Declarative eval registry entry for `spl_autoload_functions`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the SPL autoload stub.

eval_builtin! {
    name: "spl_autoload_functions",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `spl_autoload_functions` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_spl_autoload_functions(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `spl_autoload_functions` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_spl_autoload_functions_result(evaluated_args, values)
}
