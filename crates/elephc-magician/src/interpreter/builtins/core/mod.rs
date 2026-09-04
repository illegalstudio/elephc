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

mod backtrace_runtime;
mod call_user_func;
mod call_user_func_array;
mod constant;
mod define;
mod defined;
mod debug_backtrace;
mod debug_print_backtrace;
mod die;
mod error_reporting;
mod exit;
mod func_args;
mod func_get_arg;
mod func_get_args;
mod func_num_args;
mod gc_collect_cycles;
mod gc_disable;
mod gc_enable;
mod gc_enabled;
mod gc_mem_caches;
mod gc_status;
mod get_defined_constants;
mod get_defined_functions;
mod get_defined_vars;
mod get_extension_funcs;
mod get_included_files;
mod get_mangled_object_vars;
mod get_required_files;
mod get_resources;
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
mod restore_error_handler;
mod restore_exception_handler;
mod runtime_introspection;
mod set_error_handler;
mod set_exception_handler;
mod trigger_error;
mod user_error;
mod var_dump;
mod zend_version;

pub(in crate::interpreter) use call_user_func::*;
pub(in crate::interpreter) use call_user_func_array::*;
pub(in crate::interpreter) use constant::*;
pub(in crate::interpreter) use define::*;
pub(in crate::interpreter) use defined::*;
pub(in crate::interpreter) use runtime_introspection::*;
pub(in crate::interpreter) use die::*;
pub(in crate::interpreter) use exit::*;
pub(in crate::interpreter) use func_get_arg::*;
pub(in crate::interpreter) use func_get_args::*;
pub(in crate::interpreter) use func_num_args::*;
pub(in crate::interpreter) use gc_collect_cycles::*;
pub(in crate::interpreter) use gc_disable::*;
pub(in crate::interpreter) use gc_enable::*;
pub(in crate::interpreter) use gc_enabled::*;
pub(in crate::interpreter) use gc_mem_caches::*;
pub(in crate::interpreter) use gc_status::*;
pub(in crate::interpreter) use ob_get_clean::*;
pub(in crate::interpreter) use ob_get_contents::*;
pub(in crate::interpreter) use ob_get_flush::*;
pub(in crate::interpreter) use ob_get_status::*;
pub(in crate::interpreter) use ob_implicit_flush::*;
pub(in crate::interpreter) use ob_list_handlers::*;
pub(in crate::interpreter) use ob_start::*;
pub(in crate::interpreter) use print_r::*;
pub(in crate::interpreter) use var_dump::*;
pub(in crate::interpreter) use zend_version::*;

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
        "constant" => eval_builtin_constant(args, context, scope, values),
        "define" => eval_builtin_define(args, context, scope, values),
        "defined" => eval_builtin_defined(args, context, scope, values),
        "debug_backtrace" | "debug_print_backtrace" | "error_reporting"
        | "get_defined_constants" | "get_defined_functions" | "get_defined_vars"
        | "get_extension_funcs" | "get_included_files" | "get_mangled_object_vars"
        | "get_required_files" | "get_resources" | "restore_error_handler"
        | "restore_exception_handler" | "set_error_handler" | "set_exception_handler"
        | "trigger_error" | "user_error" => {
            eval_builtin_runtime_introspection_call(name, args, context, scope, values)
        }
        "die" => eval_builtin_die(args, context, scope, values),
        "exit" => eval_builtin_exit(args, context, scope, values),
        "func_get_arg" => eval_builtin_func_get_arg(args, context, scope, values),
        "func_get_args" => eval_builtin_func_get_args(args, context, scope, values),
        "func_num_args" => eval_builtin_func_num_args(args, context, values),
        "gc_collect_cycles" => eval_builtin_gc_collect_cycles(args, values),
        "gc_disable" => eval_builtin_gc_disable(args, values),
        "gc_enable" => eval_builtin_gc_enable(args, values),
        "gc_enabled" => eval_builtin_gc_enabled(args, values),
        "gc_mem_caches" => eval_builtin_gc_mem_caches(args, values),
        "gc_status" => eval_builtin_gc_status(args, values),
        "ob_get_clean" => eval_builtin_ob_get_clean(args, context, scope, values),
        "ob_get_contents" => eval_builtin_ob_get_contents(args, context, scope, values),
        "ob_get_flush" => eval_builtin_ob_get_flush(args, context, scope, values),
        "ob_get_status" => eval_builtin_ob_get_status(args, context, scope, values),
        "ob_implicit_flush" => eval_builtin_ob_implicit_flush(args, context, scope, values),
        "ob_list_handlers" => eval_builtin_ob_list_handlers(args, context, scope, values),
        "ob_start" => eval_builtin_ob_start(args, context, scope, values),
        "print_r" => eval_builtin_print_r(args, context, scope, values),
        "var_dump" => eval_builtin_var_dump(args, context, scope, values),
        "zend_version" => eval_builtin_zend_version(args, values),
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
        "constant" => eval_constant_result(evaluated_args, context, values),
        "define" => eval_define_result(evaluated_args, context, values),
        "defined" => eval_defined_result(evaluated_args, context, values),
        "debug_backtrace" | "debug_print_backtrace" | "error_reporting"
        | "get_defined_constants" | "get_defined_functions" | "get_defined_vars"
        | "get_extension_funcs" | "get_included_files" | "get_mangled_object_vars"
        | "get_required_files" | "get_resources" | "restore_error_handler"
        | "restore_exception_handler" | "set_error_handler" | "set_exception_handler"
        | "trigger_error" | "user_error" => {
            eval_runtime_introspection_values_result(name, evaluated_args, context, values)
        }
        "die" => eval_die_values_result(evaluated_args, values),
        "exit" => eval_exit_values_result(evaluated_args, values),
        "func_get_arg" => {
            eval_func_get_arg_values_result(evaluated_args, context, values)
        }
        "func_get_args" => {
            eval_func_get_args_values_result(evaluated_args, context, values)
        }
        "func_num_args" => {
            eval_func_num_args_values_result(evaluated_args, context, values)
        }
        "gc_collect_cycles" => eval_gc_collect_cycles_values_result(evaluated_args, values),
        "gc_disable" => eval_gc_disable_values_result(evaluated_args, values),
        "gc_enable" => eval_gc_enable_values_result(evaluated_args, values),
        "gc_enabled" => eval_gc_enabled_values_result(evaluated_args, values),
        "gc_mem_caches" => eval_gc_mem_caches_values_result(evaluated_args, values),
        "gc_status" => eval_gc_status_values_result(evaluated_args, values),
        "ob_get_clean" => eval_ob_get_clean_result(evaluated_args, context, values),
        "ob_get_contents" => eval_ob_get_contents_result(evaluated_args, context, values),
        "ob_get_flush" => eval_ob_get_flush_result(evaluated_args, context, values),
        "ob_get_status" => eval_ob_get_status_result(evaluated_args, context, values),
        "ob_implicit_flush" => eval_ob_implicit_flush_result(evaluated_args, context, values),
        "ob_list_handlers" => eval_ob_list_handlers_result(evaluated_args, context, values),
        "ob_start" => eval_ob_start_result(evaluated_args, context, values),
        "print_r" => eval_print_r_result(evaluated_args, context, values),
        "var_dump" => eval_var_dump_result(evaluated_args, context, values),
        "zend_version" => eval_zend_version_values_result(evaluated_args, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
