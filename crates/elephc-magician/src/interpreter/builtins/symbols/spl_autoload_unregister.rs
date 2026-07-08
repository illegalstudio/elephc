//! Purpose:
//! Eval registry entry for `spl_autoload_unregister`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Registration stub behavior is shared with `spl_autoload_register()`.

eval_builtin! {
    name: "spl_autoload_unregister",
    area: Symbols,
    params: [callback],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_unregister(...)` calls through the registration owner.
pub(in crate::interpreter) fn eval_spl_autoload_unregister_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_autoload_register::eval_builtin_spl_autoload_bool(
        "spl_autoload_unregister",
        args,
        context,
        scope,
        values,
    )
}

/// Evaluates materialized `spl_autoload_unregister(...)` arguments through the registration owner.
pub(in crate::interpreter) fn eval_spl_autoload_unregister_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_autoload_register::eval_spl_autoload_bool_result(
        "spl_autoload_unregister",
        evaluated_args,
        values,
    )
}
