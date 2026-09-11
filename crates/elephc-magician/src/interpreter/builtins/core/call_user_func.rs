//! Purpose:
//! Eval registry entry and wrapper implementation for `call_user_func`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Callable normalization and invocation stay in `registry::callable` because
//!   those helpers are shared by ordinary dynamic calls, arrays, reflection, and
//!   `call_user_func_array`.

use super::super::super::*;
use super::super::registry::eval_call_user_func_with_values_from_scope;
use super::super::registry::{eval_builtin_call_by_value, eval_builtin_uses_owned_arguments};

eval_builtin! {
    contract: "call_user_func",
    area: Core,
    source_arguments: true,
    direct: Core,
    values: Core,
}

/// Evaluates `call_user_func($name, ...$args)` inside a runtime eval fragment.
pub(in crate::interpreter) fn eval_builtin_call_user_func(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let callback = eval_owned_expr(&args[0], context, scope, values)?;
    if let Ok(EvaluatedCallable::Named { name, .. }) = eval_callable_from_scope(callback, context, scope, values) {
        if eval_builtin_uses_owned_arguments(&name) {
            let call_args = args[1..].iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
            let result = eval_builtin_call_by_value(&name, &call_args, context, scope, values);
            return release_callback_result(callback, result, context, values);
        }
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    evaluated_args.push(callback);
    for arg in &args[1..] {
        let value = match eval_expr(arg, context, scope, values) {
            Ok(value) => value,
            Err(status) => {
                return release_callback_result(callback, Err(status), context, values);
            }
        };
        evaluated_args.push(value);
    }
    let result =
        eval_call_user_func_with_values_from_scope(evaluated_args, Some(scope), context, values);
    release_callback_result(callback, result, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
pub(in crate::interpreter) fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_call_user_func_with_values_from_scope(evaluated_args, None, context, values)
}

/// Releases the captured callback owner and a successful result if that cleanup raises an exception.
pub(super) fn release_callback_result(
    callback: RuntimeCellHandle, result: Result<RuntimeCellHandle, EvalStatus>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match (result, eval_release_value(context, values, callback)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
    }
}
