//! Purpose:
//! Declarative eval registry entries and dispatch adapters for core builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` module loading.
//! - `crate::interpreter::builtins::hooks` for migrated core dispatch.
//!
//! Key details:
//! - Adapters preserve the existing direct-call helpers for lexical-scope
//!   sensitive callable dispatch and process-control behavior.

use super::super::super::*;
use super::super::*;

mod call_user_func;
mod call_user_func_array;
mod define;
mod defined;
mod die;
mod exit;
mod print_r;
mod var_dump;

/// Dispatches direct expression-level calls for declaratively migrated core builtins.
pub(in crate::interpreter) fn eval_builtin_core_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "die" | "exit" => eval_builtin_exit(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for declaratively migrated core builtins.
pub(in crate::interpreter) fn eval_core_values_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "call_user_func" => {
            eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
        }
        "define" => eval_define_result(evaluated_args, context, values),
        "defined" => eval_defined_result(evaluated_args, context, values),
        "die" | "exit" => eval_exit_result(evaluated_args, values),
        "print_r" => eval_print_r_result(evaluated_args, context, values),
        "var_dump" => eval_var_dump_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
