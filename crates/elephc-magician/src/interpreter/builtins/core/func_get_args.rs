//! Purpose:
//! Implements PHP `func_get_args()` for eval-declared callable activations.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - Each invocation returns a fresh sequential array populated from the current frame.

use super::func_args::{eval_current_function_arg, eval_throw_func_get_global_scope};
use super::super::super::*;
use crate::context::EvalFunctionArgsFrame;

eval_builtin! {
    contract: "func_get_args",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `func_get_args()` call against the current activation scope.
pub(in crate::interpreter) fn eval_builtin_func_get_args(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_func_get_args_result(context, scope, values)
}

/// Evaluates callable-dispatched `func_get_args()` against the active function frame.
pub(in crate::interpreter) fn eval_func_get_args_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let Some(scope) = context
        .current_function_args()
        .and_then(EvalFunctionArgsFrame::scope)
        .map(|scope| scope as *const ElephcEvalScope)
    else {
        return eval_throw_func_get_global_scope("func_get_args", context, values);
    };
    unsafe { eval_func_get_args_result(context, &*scope, values) }
}

/// Builds a fresh argument array from current fixed values and original positional surplus.
fn eval_func_get_args_result(
    context: &mut ElephcEvalContext,
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(frame) = context.current_function_args().cloned() else {
        return eval_throw_func_get_global_scope("func_get_args", context, values);
    };
    let mut result = values.array_new(frame.actual_count())?;
    for position in 0..frame.actual_count() {
        let key = values.int(i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        let value = eval_current_function_arg(position, &frame, scope, values)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}
