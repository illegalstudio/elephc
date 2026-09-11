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
    contract: "max",
    area: Math,
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
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(
        &operands,
        context,
        scope,
        values,
        |args, _, _, values| eval_max_result(args, values),
    )
}

/// Applies PHP `max()` to already evaluated values.
pub(in crate::interpreter) fn eval_max_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_min_max_selected(evaluated_args, EvalBinOp::Gt, values)
}
