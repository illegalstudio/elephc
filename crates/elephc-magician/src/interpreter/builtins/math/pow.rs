//! Purpose:
//! Eval registry entry and implementation for `pow`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Runtime numeric coercion and PHP edge cases stay delegated to `RuntimeValueOps`.

use super::super::super::*;

eval_builtin! {
    name: "pow",
    area: Math,
    params: [num, exponent],
    direct: Pow,
    values: Pow,
}

/// Evaluates PHP `pow()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_pow(
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
    eval_pow_result(left, right, values)
}

/// Applies PHP `pow()` to two already evaluated values.
pub(in crate::interpreter) fn eval_pow_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.pow(left, right)
}
