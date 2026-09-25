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
    if let Some(name) = eval_literal_func_args_callback(&args[0]) {
        return match name {
            "func_get_arg" => eval_builtin_func_get_arg(&args[1..], context, scope, values),
            "func_get_args" => eval_builtin_func_get_args(&args[1..], context, scope, values),
            "func_num_args" => eval_builtin_func_num_args(&args[1..], context, values),
            _ => unreachable!("literal func-args callback was canonicalized"),
        };
    }
    let callback = eval_owned_expr(&args[0], context, scope, values)?;
    if let Ok(EvaluatedCallable::Named { name, .. }) = eval_callable_from_scope(callback, context, scope, values) {
        if eval_builtin_uses_owned_arguments(&name) {
            let arguments = args[1..].iter().cloned().map(EvalCallArg::positional).collect::<Vec<_>>();
            let result = eval_builtin_call_by_value(&name, &arguments, context, scope, values);
            return finish_eval_argument_values(result, [callback], context, values);
        }
    }
    let mut operands = vec![callback];
    for argument in &args[1..] {
        match eval_owned_expr(argument, context, scope, values) {
            Ok(value) => operands.push(value),
            Err(status) => return finish_eval_argument_values(Err(status), operands, context, values),
        }
    }
    let borrowed = operands.iter().copied().map(RuntimeCellHandle::borrowed).collect();
    let result = eval_call_user_func_with_values_from_scope(borrowed, Some(scope), context, values);
    finish_eval_argument_values(result, operands, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
pub(in crate::interpreter) fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_call_user_func_with_values_from_scope(evaluated_args, None, context, values)
}
