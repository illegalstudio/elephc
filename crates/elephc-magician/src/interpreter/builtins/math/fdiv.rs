//! Purpose:
//! Eval registry entry and implementation for `fdiv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime numeric coercion and PHP edge cases stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "fdiv",
    area: Math,
    params: [num1, num2],
    direct: Fdiv,
    values: Fdiv,
}

/// Evaluates PHP `fdiv()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_fdiv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_fdiv_result(left, right, values)
}

/// Applies PHP `fdiv()` to two already evaluated values.
pub(in crate::interpreter) fn eval_fdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.fdiv(left, right)
}
