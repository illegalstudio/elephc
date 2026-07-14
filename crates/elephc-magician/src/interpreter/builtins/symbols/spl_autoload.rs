//! Purpose:
//! Eval registry entry and implementation for `spl_autoload`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Eval mirrors the main backend's conservative no-op autoload behavior.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload",
    area: Symbols,
    params: [class, file_extensions = EvalBuiltinDefaultValue::Null],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload(...)` calls as a no-op stub.
pub(in crate::interpreter) fn eval_spl_autoload_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_void("spl_autoload", args, context, scope, values)
}

/// Evaluates materialized `spl_autoload(...)` arguments as a no-op stub.
pub(in crate::interpreter) fn eval_spl_autoload_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_void_result("spl_autoload", evaluated_args, values)
}

/// Evaluates void SPL autoload call stubs.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_void(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_call" if args.len() == 1 => {}
        "spl_autoload" if (1..=2).contains(&args.len()) => {}
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    for arg in args {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_spl_autoload_void_result(name, args, values)
}

/// Evaluates materialized void SPL autoload call stubs.
pub(in crate::interpreter) fn eval_spl_autoload_void_result<T>(
    name: &str,
    evaluated_args: &[T],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_call" if evaluated_args.len() == 1 => values.null(),
        "spl_autoload" if (1..=2).contains(&evaluated_args.len()) => values.null(),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
