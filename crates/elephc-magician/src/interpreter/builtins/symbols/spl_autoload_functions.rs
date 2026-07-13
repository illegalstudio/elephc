//! Purpose:
//! Eval registry entry and implementation for `spl_autoload_functions`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval models an empty autoload function table.

eval_builtin! {
    name: "spl_autoload_functions",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_functions()` calls.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_functions(args, context, scope, values)
}

/// Evaluates materialized `spl_autoload_functions()` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_functions_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_functions_result(evaluated_args, values)
}

/// Evaluates `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_functions(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_functions_result(args, values)
}

/// Evaluates materialized `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_spl_autoload_functions_result<T>(
    evaluated_args: &[T],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.array_new(0)
}
