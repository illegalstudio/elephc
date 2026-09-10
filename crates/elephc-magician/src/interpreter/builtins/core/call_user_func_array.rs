//! Purpose:
//! Eval registry entry and wrapper implementation for `call_user_func_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Callable normalization and invocation stay in `registry::callable` because
//!   the callable engine is shared beyond this builtin.

use super::call_user_func::release_callback_result;
use super::super::super::*;
use super::super::registry::{eval_call_user_func_array_with_values_from_scope,
    eval_builtin_call_array_expr, eval_builtin_uses_owned_arguments};

eval_builtin! {
    contract: "call_user_func_array",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates `call_user_func_array($name, $args)` inside a runtime eval fragment.
pub(in crate::interpreter) fn eval_builtin_call_user_func_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback, arg_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let release_arg_array = matches!(arg_array, EvalExpr::Array(_));
    let callback = eval_owned_expr(callback, context, scope, values)?;
    if let Ok(EvaluatedCallable::Named { name, .. }) = eval_callable_from_scope(callback, context, scope, values) {
        if eval_builtin_uses_owned_arguments(&name) {
            let result = eval_builtin_call_array_expr(&name, arg_array, context, scope, values);
            return release_callback_result(callback, result, context, values);
        }
    }
    let arg_array = match eval_expr(arg_array, context, scope, values) {
        Ok(arg_array) => arg_array,
        Err(status) => {
            return release_callback_result(callback, Err(status), context, values);
        }
    };
    let result = eval_call_user_func_array_with_values_from_scope(
        callback,
        arg_array,
        Some(scope),
        context,
        values,
    );
    if release_arg_array {
        if let Err(status) = eval_release_value(context, values, arg_array) {
            if let Ok(value) = result { let _ = eval_release_value(context, values, value); }
            return release_callback_result(callback, Err(status), context, values);
        }
    }
    release_callback_result(callback, result, context, values)
}

/// Dispatches `call_user_func_array` after callback and array arguments are evaluated.
pub(in crate::interpreter) fn eval_call_user_func_array_with_values(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_call_user_func_array_with_values_from_scope(callback, arg_array, None, context, values)
}
