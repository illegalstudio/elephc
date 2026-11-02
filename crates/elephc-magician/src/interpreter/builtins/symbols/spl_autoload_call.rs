//! Purpose:
//! Eval registry entry for `spl_autoload_call`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Registered callbacks execute through the shared SPL autoload dispatcher.

eval_builtin! {
    contract: "spl_autoload_call",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_call(...)` calls through the `spl_autoload` owner.
pub(in crate::interpreter) fn eval_spl_autoload_call_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_autoload::eval_builtin_spl_autoload_void(
        "spl_autoload_call",
        args,
        context,
        scope,
        values,
    )
}

/// Evaluates materialized `spl_autoload_call(...)` arguments through the `spl_autoload` owner.
pub(in crate::interpreter) fn eval_spl_autoload_call_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::spl_autoload::eval_spl_autoload_void_result(
        "spl_autoload_call",
        evaluated_args,
        context,
        values,
    )
}
