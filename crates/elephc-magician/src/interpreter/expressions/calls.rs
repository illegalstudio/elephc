//! Purpose:
//! Evaluates function-like EvalIR calls and first-class callable expressions.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_expr()` for call-shaped expressions.
//!
//! Key details:
//! - Source-sensitive constructs and by-reference builtins keep unevaluated or
//!   ref-target arguments before ordinary registry direct-call dispatch.
//! - Dynamic callables preserve PHP source-order argument evaluation before
//!   normalized callable invocation.

use super::*;

mod first_class;
mod first_class_support;

pub(in crate::interpreter) use first_class::*;
use first_class_support::*;

/// Returns cloned positional argument expressions, rejecting named arguments.
pub(in crate::interpreter) fn positional_call_arg_exprs(
    args: &[EvalCallArg],
) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates method-call arguments, preserving named metadata for eval method binding.
pub(in crate::interpreter) fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    eval_call_arg_values(args, context, scope, values)
}

/// Evaluates supported function-like calls from a runtime eval fragment.
pub(in crate::interpreter) fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if name == "flock" {
        return eval_builtin_flock(args, context, scope, values);
    }
    if name == "proc_open" {
        return eval_builtin_proc_open_call(args, context, scope, values);
    }
    if name == "preg_match" {
        return eval_builtin_preg_match_call(args, context, scope, values);
    }
    if name == "preg_match_all" {
        return eval_builtin_preg_match_all_call(args, context, scope, values);
    }
    if name == "is_callable" {
        return eval_builtin_is_callable_call(args, context, scope, values);
    }
    if matches!(name, "fsockopen" | "pfsockopen") {
        return eval_builtin_fsockopen_call(args, context, scope, values);
    }
    if let Some(result) = eval_date_procedural_alias_call(name, args, context, scope, values)? {
        return Ok(result);
    }
    if name == "stream_select" {
        return eval_builtin_stream_select_call(args, context, scope, values);
    }
    if name == "stream_socket_accept" {
        return eval_builtin_stream_socket_accept_call(args, context, scope, values);
    }
    if name == "stream_socket_recvfrom" {
        return eval_builtin_stream_socket_recvfrom_call(args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "array_walk"
            | "arsort"
            | "asort"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "rsort"
            | "shuffle"
            | "sort"
            | "settype"
            | "uasort"
            | "uksort"
            | "usort"
    ) {
        return eval_builtin_array_mutating_declared_call(name, args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates an unqualified namespaced function call with PHP's global fallback.
pub(in crate::interpreter) fn eval_namespaced_call(
    name: &str,
    fallback_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    eval_call(fallback_name, args, context, scope, values)
}

/// Evaluates a variable or expression callable and dispatches it with source-order arguments.
pub(in crate::interpreter) fn eval_dynamic_call(
    callee: &EvalExpr,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = eval_expr(callee, context, scope, values)?;
    if values.type_tag(callback)? == EVAL_TAG_OBJECT {
        let is_closure_object = values
            .object_identity(callback)
            .ok()
            .and_then(|identity| context.closure_object_target(identity))
            .is_some();
        if !is_closure_object {
            eval_invokable_object_precheck(callback, context, values)?;
            let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
            return eval_invokable_object_call_result(callback, evaluated_args, context, values);
        }
    }
    let callback = eval_callable(callback, context, values)?;
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    eval_evaluated_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Returns true for language constructs that need unevaluated argument expressions.
pub(in crate::interpreter) fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset" | "unset")
}

/// Returns true when every source argument is plain positional.
pub(in crate::interpreter) fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates registry-backed direct builtins and language constructs after positional-only validation.
pub(in crate::interpreter) fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "eval" {
        return eval_nested_eval(args, context, scope, values);
    }

    if let Some(result) = eval_declared_builtin_direct_call(name, args, context, scope, values)? {
        return Ok(result);
    }

    Err(EvalStatus::UnsupportedConstruct)
}
