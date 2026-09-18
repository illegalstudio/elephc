//! Purpose:
//! Eval registry entry and wrapper implementation for `call_user_func_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Callable normalization and invocation stay in `registry::callable` because
//!   the callable engine is shared beyond this builtin.

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
    with_eval_operands(&[callback, arg_array], context, scope, values, |args, context, scope, values| {
        eval_call_user_func_array_with_values_from_scope(
            args[0], args[1], Some(scope), context, values,
        )
    })
}

/// Invokes a literal `func_*` callback using one runtime `call_user_func_array` argument list.
fn eval_literal_func_args_array_call(
    name: &str,
    arg_array: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_operands(&[arg_array], context, scope, values, |args, context, _, values| {
        let arg_array = args[0];
        if !values.is_array_like(arg_array)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        with_eval_array_call_arguments(arg_array, context, values, |arguments, context, values| {
            if arguments.iter().any(|arg| {
                arg.name.as_deref()
                    .is_some_and(|argument| name != "func_get_arg" || argument != "position")
            }) {
                return Err(EvalStatus::RuntimeFatal);
            }
            let evaluated_values = arguments.iter().map(|arg| arg.value).collect::<Vec<_>>();
            match name {
                "func_get_arg" => eval_func_get_arg_values_result(&evaluated_values, context, values),
                "func_get_args" => eval_func_get_args_values_result(&evaluated_values, context, values),
                "func_num_args" => eval_func_num_args_values_result(&evaluated_values, context, values),
                _ => unreachable!("literal func-args callback was canonicalized"),
            }
        })
    })
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
