//! Purpose:
//! Implements PHP `func_get_arg()` for eval-declared callable activations.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Position validation uses PHP's two distinct catchable `ValueError` diagnostics.

use super::func_args::{eval_current_function_arg, eval_throw_func_get_global_scope};
use super::super::super::*;
use crate::context::EvalFunctionArgsFrame;

const NEGATIVE_POSITION_MESSAGE: &str =
    "func_get_arg(): Argument #1 ($position) must be greater than or equal to 0";
const OUT_OF_RANGE_POSITION_MESSAGE: &str =
    "func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function";

eval_builtin! {
    contract: "func_get_arg",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `func_get_arg()` call in source order.
pub(in crate::interpreter) fn eval_builtin_func_get_arg(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [position] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let position = eval_expr(position, context, scope, values)?;
    eval_func_get_arg_result(position, context, scope, values)
}

/// Evaluates callable-dispatched `func_get_arg()` against the active function frame.
pub(in crate::interpreter) fn eval_func_get_arg_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [position] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let Some(scope) = context
        .current_function_args()
        .and_then(EvalFunctionArgsFrame::scope)
        .map(|scope| scope as *const ElephcEvalScope)
    else {
        return eval_throw_func_get_global_scope("func_get_arg", context, values);
    };
    unsafe { eval_func_get_arg_result(*position, context, &*scope, values) }
}

/// Validates one position and returns the corresponding current argument value.
fn eval_func_get_arg_result(
    position: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(frame) = context.current_function_args().cloned() else {
        return eval_throw_func_get_global_scope("func_get_arg", context, values);
    };
    let position = eval_int_value(position, values)?;
    if position < 0 {
        return eval_throw_builtin_value_error(NEGATIVE_POSITION_MESSAGE, context, values);
    }
    let position = usize::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
    if position >= frame.actual_count() {
        return eval_throw_builtin_value_error(OUT_OF_RANGE_POSITION_MESSAGE, context, values);
    }
    eval_current_function_arg(position, &frame, scope, values)
}
