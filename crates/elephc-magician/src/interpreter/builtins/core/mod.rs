//! Purpose:
//! Orchestrates eval metadata and implementations for core callable, constant,
//! process-control, and debug-output builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by registry hooks.
//!
//! Key details:
//! - Leaf builtin files own their declarations and builtin-specific wrappers.
//! - The callable dispatch engine remains shared because it is used by more than
//!   `call_user_func*`.

use super::super::*;

mod call_user_func;
mod call_user_func_array;
mod define;
mod defined;
mod die;
mod exit;
mod ob_clean;
mod ob_end_clean;
mod ob_end_flush;
mod ob_flush;
mod ob_get_clean;
mod ob_get_contents;
mod ob_get_flush;
mod ob_get_length;
mod ob_get_level;
mod ob_get_status;
mod ob_implicit_flush;
mod ob_list_handlers;
mod ob_start;
mod print_r;
mod var_dump;

pub(in crate::interpreter) use call_user_func::*;
pub(in crate::interpreter) use call_user_func_array::*;
pub(in crate::interpreter) use define::*;
pub(in crate::interpreter) use defined::*;
pub(in crate::interpreter) use die::*;
pub(in crate::interpreter) use exit::*;
pub(in crate::interpreter) use ob_clean::*;
pub(in crate::interpreter) use ob_end_clean::*;
pub(in crate::interpreter) use ob_end_flush::*;
pub(in crate::interpreter) use ob_flush::*;
pub(in crate::interpreter) use ob_get_clean::*;
pub(in crate::interpreter) use ob_get_contents::*;
pub(in crate::interpreter) use ob_get_flush::*;
pub(in crate::interpreter) use ob_get_length::*;
pub(in crate::interpreter) use ob_get_level::*;
pub(in crate::interpreter) use ob_get_status::*;
pub(in crate::interpreter) use ob_implicit_flush::*;
pub(in crate::interpreter) use ob_list_handlers::*;
pub(in crate::interpreter) use ob_start::*;
pub(in crate::interpreter) use print_r::*;
pub(in crate::interpreter) use var_dump::*;

/// Dispatches direct expression-level calls for core builtins.
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
        "die" => eval_builtin_die(args, context, scope, values),
        "exit" => eval_builtin_exit(args, context, scope, values),
        "ob_clean" => eval_builtin_ob_clean(args, context, scope, values),
        "ob_end_clean" => eval_builtin_ob_end_clean(args, context, scope, values),
        "ob_end_flush" => eval_builtin_ob_end_flush(args, context, scope, values),
        "ob_flush" => eval_builtin_ob_flush(args, context, scope, values),
        "ob_get_clean" => eval_builtin_ob_get_clean(args, context, scope, values),
        "ob_get_contents" => eval_builtin_ob_get_contents(args, context, scope, values),
        "ob_get_flush" => eval_builtin_ob_get_flush(args, context, scope, values),
        "ob_get_length" => eval_builtin_ob_get_length(args, context, scope, values),
        "ob_get_level" => eval_builtin_ob_get_level(args, context, scope, values),
        "ob_get_status" => eval_builtin_ob_get_status(args, context, scope, values),
        "ob_implicit_flush" => eval_builtin_ob_implicit_flush(args, context, scope, values),
        "ob_list_handlers" => eval_builtin_ob_list_handlers(args, context, scope, values),
        "ob_start" => eval_builtin_ob_start(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Dispatches evaluated-argument calls for core builtins.
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
        "die" => eval_die_values_result(evaluated_args, values),
        "exit" => eval_exit_values_result(evaluated_args, values),
        "ob_clean" => eval_ob_clean_result(evaluated_args, context, values),
        "ob_end_clean" => eval_ob_end_clean_result(evaluated_args, context, values),
        "ob_end_flush" => eval_ob_end_flush_result(evaluated_args, context, values),
        "ob_flush" => eval_ob_flush_result(evaluated_args, context, values),
        "ob_get_clean" => eval_ob_get_clean_result(evaluated_args, context, values),
        "ob_get_contents" => eval_ob_get_contents_result(evaluated_args, context, values),
        "ob_get_flush" => eval_ob_get_flush_result(evaluated_args, context, values),
        "ob_get_length" => eval_ob_get_length_result(evaluated_args, context, values),
        "ob_get_level" => eval_ob_get_level_result(evaluated_args, context, values),
        "ob_get_status" => eval_ob_get_status_result(evaluated_args, context, values),
        "ob_implicit_flush" => eval_ob_implicit_flush_result(evaluated_args, context, values),
        "ob_list_handlers" => eval_ob_list_handlers_result(evaluated_args, context, values),
        "ob_start" => eval_ob_start_result(evaluated_args, context, values),
        "print_r" => eval_print_r_result(evaluated_args, context, values),
        "var_dump" => eval_var_dump_result(evaluated_args, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
