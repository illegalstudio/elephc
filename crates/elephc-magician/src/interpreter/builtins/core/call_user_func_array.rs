//! Purpose:
//! Eval registry entry and wrapper implementation for `call_user_func_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Callable normalization and invocation stay in `registry::callable` because
//!   the callable engine is shared beyond this builtin.

use super::call_user_func::eval_call_user_func_callback_expr_is_temporary;
use super::super::super::*;
use super::super::registry::eval_call_user_func_array_with_values_from_scope;

eval_builtin! {
    name: "call_user_func_array",
    area: Core,
    params: [callback, args],
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
    let release_callback = eval_call_user_func_callback_expr_is_temporary(callback);
    let release_arg_array = matches!(arg_array, EvalExpr::Array(_));
    let callback = eval_expr(callback, context, scope, values)?;
    let arg_array = match eval_expr(arg_array, context, scope, values) {
        Ok(arg_array) => arg_array,
        Err(status) => {
            if release_callback {
                values.release(callback)?;
            }
            return Err(status);
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
        values.release(arg_array)?;
    }
    if release_callback {
        values.release(callback)?;
    }
    result
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
