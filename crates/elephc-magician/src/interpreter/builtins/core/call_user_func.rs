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
use super::func_args::eval_literal_func_args_callback;

eval_builtin! {
    contract: "call_user_func",
    area: Core,
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
    if let Some(name) = eval_literal_func_args_callback(&args[0]) {
        return match name {
            "func_get_arg" => eval_builtin_func_get_arg(&args[1..], context, scope, values),
            "func_get_args" => eval_builtin_func_get_args(&args[1..], context, scope, values),
            "func_num_args" => eval_builtin_func_num_args(&args[1..], context, values),
            _ => unreachable!("literal func-args callback was canonicalized"),
        };
    }
    let release_callback = eval_call_user_func_callback_expr_is_temporary(&args[0]);
    let mut evaluated_args = Vec::with_capacity(args.len());
    for (index, arg) in args.iter().enumerate() {
        let value = match eval_expr(arg, context, scope, values) {
            Ok(value) => value,
            Err(status) => {
                if index > 0 && release_callback {
                    values.release(evaluated_args[0])?;
                }
                return Err(status);
            }
        };
        evaluated_args.push(value);
    }
    let callback = evaluated_args[0];
    let result =
        eval_call_user_func_with_values_from_scope(evaluated_args, Some(scope), context, values);
    if release_callback {
        values.release(callback)?;
    }
    result
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
pub(in crate::interpreter) fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_call_user_func_with_values_from_scope(evaluated_args, None, context, values)
}

/// Returns whether a `call_user_func*` callback expression allocates a temporary cell.
pub(in crate::interpreter) fn eval_call_user_func_callback_expr_is_temporary(
    callback: &EvalExpr,
) -> bool {
    matches!(callback, EvalExpr::Const(_))
}
