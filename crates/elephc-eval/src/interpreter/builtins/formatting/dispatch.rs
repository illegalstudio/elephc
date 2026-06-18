//! Purpose:
//! Evaluates direct printf-family eval arguments before dispatching to formatted
//! result helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` builtin dispatch paths.
//!
//! Key details:
//! - Argument expressions are evaluated once in source order before builtin-family
//!   selection is applied.

use super::super::super::*;
use super::*;

/// Evaluates direct positional `sprintf()` or `printf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_sprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_sprintf_like_result(name, &evaluated_args, values)
}

/// Evaluates direct positional `vsprintf()` or `vprintf()` calls in source order.
pub(in crate::interpreter) fn eval_builtin_vsprintf_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_vsprintf_like_result(name, &evaluated_args, values)
}

/// Dispatches already evaluated `sprintf()` or `printf()` arguments.
pub(in crate::interpreter) fn eval_sprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "sprintf" => eval_sprintf_result(evaluated_args, values),
        "printf" => eval_printf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Dispatches already evaluated `vsprintf()` or `vprintf()` arguments.
pub(in crate::interpreter) fn eval_vsprintf_like_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "vsprintf" => eval_vsprintf_result(evaluated_args, values),
        "vprintf" => eval_vprintf_result(evaluated_args, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
