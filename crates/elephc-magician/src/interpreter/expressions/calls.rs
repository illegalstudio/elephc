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
    // Ahead of every specialised handler below: each one used to refuse a wrong count with a
    // `RuntimeFatal` of its own, where PHP throws a catchable `ArgumentCountError`.
    eval_check_builtin_source_arity(name, args, context, scope, values)?;
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
    if name == "openssl_encrypt" {
        return eval_builtin_openssl_encrypt_call(args, context, scope, values);
    }
    if name == "is_callable" {
        return eval_builtin_is_callable_call(args, context, scope, values);
    }
    if matches!(name, "fsockopen" | "pfsockopen") {
        return eval_builtin_fsockopen_call(args, context, scope, values);
    }
    // The two by-reference curl builtins, intercepted here for the same reason every other
    // by-reference builtin above is: `eval_positional_expr_call` hands its hook only
    // `&[EvalExpr]`, which has already lost named-argument metadata, and the by-value
    // dispatchers have no reference targets at all. See
    // `crate::interpreter::builtins::curl::curl_multi_exec`'s header.
    #[cfg(feature = "curl")]
    if name == "curl_multi_exec" {
        return eval_builtin_curl_multi_exec_call(args, context, scope, values);
    }
    #[cfg(feature = "curl")]
    if name == "curl_multi_info_read" {
        return eval_builtin_curl_multi_info_read_call(args, context, scope, values);
    }
    if name.starts_with("pcntl_") && eval_php_visible_builtin_exists(name) {
        return eval_builtin_pcntl_call(name, args, context, scope, values);
    }
    // The xml surface forwards to the host's compiled prelude; the source-level path keeps
    // `xml_parse_into_struct()`'s by-reference outputs and named arguments intact.
    if eval_xml_builtin_name(name) {
        return eval_builtin_xml_call(name, args, context, scope, values);
    }
    // ALL EIGHT OPcache functions are prelude-provided on the native side (they are not
    // catalog builtins), so eval carries a fallback handler for each. Every one of those
    // handlers is guarded the same way, and the uniformity is the point rather than a
    // stylistic preference.
    //
    // When the binary DOES carry the prelude declaration, that declaration is the only thing
    // that knows the compile-time manifest, the live runtime script cache, the resolved
    // `--ini` values and `ini_set()` overrides — and `opcache.restrict_api`, which the
    // prelude implements as a guard at the top of each body. A handler that intercepts ahead
    // of it does not merely answer with stale data: it answers INSTEAD of the refusal. With
    // `opcache.restrict_api=/nonexistent`, native `opcache_reset()` warns and returns
    // `false` while an eval'd `opcache_reset()` returned `true` and scheduled a real flush,
    // so `eval()` was a way around the directive for seven of the eight names.
    //
    // So `opcache_direct` hands the call to that declaration when it exists, and to the
    // by-values dispatch `call_user_func()` reaches when it does not — in both cases after
    // binding the arguments to the reference parameters and checking the internal-function
    // count, which the declaration's userland signature would answer differently.
    if let Some(parameters) = eval_opcache_parameters(name, context) {
        return eval_opcache_direct_call(name, parameters, args, context, scope, values);
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
            | "end"
            | "krsort"
            | "ksort"
            | "natcasesort"
            | "natsort"
            | "next"
            | "prev"
            | "reset"
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
        if eval_call_args_are_plain_positional(args)
            && eval_declared_builtin_requires_source_arguments(name)
        {
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
    with_eval_operands(&[callee], context, scope, values, |callbacks, context, scope, values| {
        eval_dynamic_call_with_callback(callbacks[0], args, context, scope, values)
    })
}

/// Invokes a rooted callback while source-order argument leases protect all consumed values.
fn eval_dynamic_call_with_callback(
    callback: RuntimeCellHandle,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(callback)? == EVAL_TAG_OBJECT {
        let is_closure_object = values
            .object_identity(callback)
            .ok()
            .and_then(|identity| context.closure_object_target(identity))
            .is_some();
        let is_detached_pcntl_handler = context
            .pcntl_foreign_callable_owner(callback)
            .is_some()
            || crate::context::pcntl_runtime::is_handler_callable(callback);
        if !is_closure_object && !is_detached_pcntl_handler {
            eval_invokable_object_precheck(callback, context, values)?;
            return with_eval_call_arguments(args, context, scope, values, |arguments, context, _, values| {
                eval_invokable_object_call_result(callback, arguments, context, values)
            });
        }
    }
    let callback = eval_callable(callback, context, values)?;
    with_eval_call_arguments(args, context, scope, values, |arguments, context, _, values| {
        eval_evaluated_callable_with_call_array_args(&callback, arguments, context, values)
    })
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
