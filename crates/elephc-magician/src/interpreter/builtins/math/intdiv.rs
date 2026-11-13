//! Purpose:
//! Eval registry entry and implementation for `intdiv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Value-domain failures use the ordinary context-owned Throwable channel.

use super::super::super::*;

eval_builtin! {
    contract: "intdiv",
    area: Math,
    direct: Intdiv,
    values: Intdiv,
}

/// Evaluates PHP `intdiv()` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_intdiv(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_eval_operands(&[left, right], context, scope, values,
        |args, context, _, values| eval_intdiv_result(args[0], args[1], context, values))
}

/// Applies PHP `intdiv()` to two already evaluated values.
pub(in crate::interpreter) fn eval_intdiv_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let left = eval_int_value(left, values)?;
    let right = eval_int_value(right, values)?;
    if right == 0 {
        return throwables::eval_throw_builtin_division_by_zero_error("Division by zero", context, values);
    }
    match left.checked_div(right) {
        Some(result) => values.int(result),
        None => throwables::eval_throw_builtin_exception("ArithmeticError",
            elephc_semantics::numeric::INTDIV_OVERFLOW_MESSAGE, context, values),
    }
}
