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
use std::borrow::Cow;
use crate::context::decode_eval_callback_adapter_capture;

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
    eval_call_with_error_name(name, name, args, context, scope, values)
}

/// Normalizes mixed-case lookup keys without allocating for ordinary lowercase calls.
fn call_lookup_name(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

/// Keeps source spelling for errors while resolving a case-insensitive call target.
fn eval_call_with_error_name(
    name: &str,
    error_name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let lookup_name = call_lookup_name(name);
    let name = lookup_name.as_ref();
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
    if name == "preg_match" {
        return eval_builtin_preg_match_call(args, context, scope, values);
    }
    if name == "preg_match_all" {
        return eval_builtin_preg_match_all_call(args, context, scope, values);
    }
    if name == "preg_replace" {
        return eval_builtin_preg_replace_call(args, context, scope, values);
    }
    if name == "preg_replace_callback" {
        return eval_builtin_preg_replace_callback_call(args, context, scope, values);
    }
    if name == "preg_filter" {
        return eval_builtin_preg_filter_call(args, context, scope, values);
    }
    if name == "preg_replace_callback_array" {
        return eval_builtin_preg_replace_callback_array_call(args, context, scope, values);
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
    // `proc_open()` writes `$pipes` through the caller's own cell, so it keeps its source
    // arguments for the same reason `fsockopen` does.
    if name == "proc_open" {
        return eval_builtin_proc_open_call(args, context, scope, values);
    }
    // `exec($command, &$output, &$result_code)`: the one-argument form stays on the registry
    // path; the by-reference forms need their reference targets.
    if name == "exec" && args.len() > 1 {
        return eval_builtin_exec_call(args, context, scope, values);
    }
    // Same shape as `fsockopen` above, and intercepted for the same reason: PHP writes
    // `$error_code`/`$error_message` through the caller's own cells, and only this path still
    // holds them.
    if name == "stream_socket_client" {
        return eval_builtin_stream_socket_client_call(args, context, scope, values);
    }
    // `headers_sent($file, $line)` (Symfony's `NativeSessionStorage::start()`): the evaluated-
    // argument route reads the two outputs as values and writes nothing back, so a call that
    // passes them keeps its source arguments and reaches the direct hook that binds them.
    if name == "headers_sent" && !args.is_empty() && eval_call_args_are_plain_positional(args) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    // `debug_backtrace` and `debug_print_backtrace` describe interpreter frames, which only
    // the interpreter has, so they are dispatched as plain runtime handlers rather than
    // through the PHP-visible builtin registry. See `builtins::core::debug_backtrace`.
    if matches!(name, "debug_backtrace" | "debug_print_backtrace") {
        return eval_debug_backtrace_call(name, args, context, scope, values);
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
    if eval_fileinfo_builtin_name(name) {
        return eval_builtin_fileinfo_call(name, args, context, scope, values);
    }
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
    if name == "parse_str" {
        return eval_builtin_parse_str_call(args, context, scope, values);
    }
    if matches!(
        name,
        "array_pop"
            | "array_push"
            | "array_shift"
            | "array_splice"
            | "array_unshift"
            | "array_walk"
            | "array_walk_recursive"
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
            && (eval_declared_builtin_requires_source_arguments(name)
                || eval_declared_builtin_call_binds_reference(name, args.len()))
        {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    // Answered before the lookup: `compact()` is not a function anywhere, in php or here. The
    // compiler desugars it at compile time, so there is nothing to find in any table.
    if eval_is_compact_intrinsic(name) {
        return eval_compact_intrinsic(args, context, scope, values);
    }
    let (base, written) = split_generic_name(name)?;
    if let Some(template) = context.generic_function_template(&base).cloned() {
        if matches!(template.prototype.as_ref(), EvalStmt::FunctionDecl { .. }) {
            return eval_generic_function(&template, written.as_deref(), args, context, scope, values);
        }
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(name, function, args, context, scope, values);
    }
    // Last: a function declared by an include that ran inside ANOTHER eval context. Composer's
    // `files` autoload requires its shims from inside a bound closure, and every compiled frame
    // that includes gets its own context, so `trigger_deprecation()` lives in one this call has
    // never seen. See `ElephcEvalContext::adopt_global_function`.
    if context.adopt_global_function(name) {
        if let Some(function) = context.function(name).cloned() {
            return eval_dynamic_function(&function, args, context, scope, values);
        }
    }
    if name.eq_ignore_ascii_case("assert") {
        return eval_assert_call(args, context, scope, values);
    }
    // A known builtin may require an unavailable prelude or capability. Preserve
    // that diagnostic; an undefined function is a catchable PHP Error.
    if elephc_builtin_contract::lookup(name).is_some() {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    eval_throw_error(
        &format!("Call to undefined function {}()", error_name.trim_start_matches('\\')),
        context,
        values,
    )
}

/// Evaluates `assert($assertion[, $description])` the way PHP's default `zend.assertions=1` does.
///
/// The compiler turns `assert()` into an injected helper, so there is no builtin to find, and an
/// interpreted body reached "call to undefined function assert()" -- Symfony's
/// `TagAwareAdapter::invalidateTags()` guards itself with one, which killed
/// `cache:pool:invalidate-tags`. A failing assertion throws its description when that is a
/// Throwable, and otherwise an `AssertionError` carrying it (or the compiled helper's message).
fn eval_assert_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let args = positional_call_arg_exprs(args)?;
    let Some(assertion) = args.first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if eval_truthy_expr(assertion, context, scope, values)? {
        return values.bool_value(true);
    }
    let message = match args.get(1) {
        Some(description) => {
            let value = eval_expr(description, context, scope, values)?;
            if values.type_tag(value)? == EVAL_TAG_OBJECT {
                context.set_pending_throw(value);
                return Err(EvalStatus::UncaughtThrowable);
            }
            let message = values.cast_string(value);
            if !value.is_borrowed() {
                eval_release_value(context, values, value)?;
            }
            message?
        }
        None => values.string("assertion failed")?,
    };
    let exception = values.new_object("AssertionError")?;
    let code = values.int(0)?;
    // The constructor copies its operands; the message and code cells stay ours to release.
    let constructed = values.construct_object(exception, vec![message, code]);
    let released = eval_release_value(context, values, code)
        .and(eval_release_value(context, values, message));
    constructed.and(released)?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
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
    let lookup_name = call_lookup_name(name);
    let (base, written) = split_generic_name(&lookup_name)?;
    if let Some(template) = context.generic_function_template(&base).cloned() {
        if matches!(template.prototype.as_ref(), EvalStmt::FunctionDecl { .. }) {
            return eval_generic_function(&template, written.as_deref(), args, context, scope, values);
        }
    }
    if let Some(function) = context.function(&lookup_name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(&lookup_name) {
        return eval_native_function(name, function, args, context, scope, values);
    }
    eval_call_with_error_name(fallback_name, name, args, context, scope, values)
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
    let callback_tag = values.type_tag(callback)?;
    if crate::eval_trace::enabled() {
        eprintln!("[elephc-eval-trace] phase=dynamic_call callback_tag={callback_tag}");
    }
    if callback_tag == EVAL_TAG_CALLABLE {
        let descriptor = values.raw_value_word(callback)? as usize as *mut std::ffi::c_void;
        if crate::eval_trace::enabled() {
            let word0 = (descriptor as usize >= 4096)
                .then(|| unsafe { *(descriptor as *const u64) });
            eprintln!(
                "[elephc-eval-trace] phase=dynamic_call descriptor={descriptor:p} word0={word0:?}",
            );
        }
        if let Some(decoded) = unsafe { decode_callable_descriptor(descriptor) } {
            if crate::eval_trace::enabled() {
                eprintln!("[elephc-eval-trace] phase=dynamic_call descriptor=direct");
            }
            // A compiled closure carries the compiler's internal symbol as its name; php's frame
            // calls it `{closure}`.
            let frame_name = if decoded.display_name.starts_with("__eir_closure") {
                "{closure}"
            } else {
                decoded.display_name.as_str()
            };
            return eval_native_function(frame_name, decoded.function, args, context, scope, values);
        }
        if let Some((captured_context, captured)) =
            unsafe { decode_eval_callback_adapter_capture(descriptor.cast()) }
        {
            let captured = if captured.type_tag == EVAL_TAG_MIXED {
                RuntimeCellHandle::from_raw(
                    captured.value_word as usize as *mut crate::value::RuntimeCell,
                )
                .borrowed()
            } else {
                values.raw_word_value(captured.type_tag, captured.value_word)?
            };
            let captured_context = unsafe {
                (captured_context as usize as *mut ElephcEvalContext).as_mut()
            }
            .ok_or(EvalStatus::RuntimeFatal)?;
            let callback = eval_callable(captured, captured_context, values)?;
            return with_eval_call_arguments(args, context, scope, values, |evaluated_args, _, _, values| {
                callback.with_values(captured_context, values, |callback, captured_context, values| {
                    eval_evaluated_callable_with_call_array_args(
                        callback,
                        evaluated_args,
                        captured_context,
                        values,
                    )
                })
            });
        }
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if callback_tag == EVAL_TAG_OBJECT {
        let is_closure_object = values
            .object_identity(callback)
            .ok()
            .and_then(|identity| context.closure_object_target(identity))
            .is_some()
            || eval_foreign_closure_owner(callback, context, values)?.is_some();
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
    callback.with_values(context, values, |callback, context, values| {
        with_eval_call_arguments(args, context, scope, values, |arguments, context, _, values| {
            eval_evaluated_callable_with_call_array_args(&callback, arguments, context, values)
        })
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

    note_eval_runtime_failure(
        format!("call to unsupported builtin {name}()"),
        context,
    );
    Err(EvalStatus::UnsupportedConstruct)
}
