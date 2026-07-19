//! Purpose:
//! Eval registry entry and implementation for `max`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Variadic inputs are evaluated in PHP source order before runtime comparison.

use super::super::super::*;

eval_builtin! {
    name: "max",
    area: Math,
    params: [value],
    variadic: values,
    direct: Max,
    values: Max,
}

/// Evaluates PHP `max()` over two or more eval expressions.
pub(in crate::interpreter) fn eval_builtin_max(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_max_result(&evaluated_args, values)
}

/// Applies PHP `max()` to already evaluated values.
pub(in crate::interpreter) fn eval_max_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_min_max_selected(evaluated_args, EvalBinOp::Gt, values)
}
