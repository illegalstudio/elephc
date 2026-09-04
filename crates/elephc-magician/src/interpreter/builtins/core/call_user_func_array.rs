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
use super::func_args::eval_literal_func_args_callback;

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
    if let Some(name) = eval_literal_func_args_callback(callback) {
        return eval_literal_func_args_array_call(name, arg_array, context, scope, values);
    }
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

/// Invokes a literal `func_*` callback using one runtime `call_user_func_array` argument list.
fn eval_literal_func_args_array_call(
    name: &str,
    arg_array: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let release_arg_array = matches!(arg_array, EvalExpr::Array(_));
    let arg_array = eval_expr(arg_array, context, scope, values)?;
    let result = (|| {
        if !values.is_array_like(arg_array)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_args = eval_array_call_arg_values(arg_array, context, values)?;
        if evaluated_args.iter().any(|arg| {
            arg.name
                .as_deref()
                .is_some_and(|argument| name != "func_get_arg" || argument != "position")
        }) {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_values = evaluated_args
            .iter()
            .map(|arg| arg.value)
            .collect::<Vec<_>>();
        match name {
            "func_get_arg" => eval_func_get_arg_values_result(&evaluated_values, context, values),
            "func_get_args" => eval_func_get_args_values_result(&evaluated_values, context, values),
            "func_num_args" => eval_func_num_args_values_result(&evaluated_values, context, values),
            _ => unreachable!("literal func-args callback was canonicalized"),
        }
    })();
    if release_arg_array {
        values.release(arg_array)?;
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
