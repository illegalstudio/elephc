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

eval_builtin! {
    name: "call_user_func",
    area: Core,
    params: [callback],
    variadic: args,
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
