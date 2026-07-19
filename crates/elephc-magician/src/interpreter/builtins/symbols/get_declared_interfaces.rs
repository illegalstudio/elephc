//! Purpose:
//! Eval registry entry for `get_declared_interfaces`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Array construction is shared with `get_declared_classes()`.

eval_builtin! {
    name: "get_declared_interfaces",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_declared_interfaces()` calls.
pub(in crate::interpreter) fn eval_get_declared_interfaces_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::get_declared_classes::eval_builtin_get_declared_symbols(
        "get_declared_interfaces",
        args,
        context,
        values,
    )
}

/// Evaluates materialized `get_declared_interfaces()` arguments.
pub(in crate::interpreter) fn eval_get_declared_interfaces_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        super::get_declared_classes::eval_get_declared_symbols_result(
            "get_declared_interfaces",
            context,
            values,
        )
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}
