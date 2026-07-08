//! Purpose:
//! Eval registry entry and implementation for `min`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Variadic inputs are evaluated in PHP source order before runtime comparison.

use super::super::super::*;

eval_builtin! {
    name: "min",
    area: Math,
    params: [value],
    variadic: values,
    direct: Min,
    values: Min,
}

/// Evaluates PHP `min()` over two or more eval expressions.
pub(in crate::interpreter) fn eval_builtin_min(
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
    eval_min_result(&evaluated_args, values)
}

/// Applies PHP `min()` to already evaluated values.
pub(in crate::interpreter) fn eval_min_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_min_max_selected(evaluated_args, EvalBinOp::Lt, values)
}
