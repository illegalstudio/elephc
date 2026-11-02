//! Purpose:
//! Executes generated/AOT instance and static methods through native bridge scopes.
//!
//! Called from:
//! - Method, static-method, Reflection, and call_user_func dispatch.
//!
//! Key details:
//! - Checked and unchecked entry points share binding, bridge scope, and reference-mode handling.

use super::*;

/// Records a compiled method activation as a live call frame so `debug_backtrace()` and the
/// throwable trace see the eval-to-compiled boundary the way PHP shows every call.
///
/// Returns whether a frame was pushed, which the caller pairs with `pop_call_frame()` after the
/// compiled body returns. Without it the frame whose `file` and `line` describe the eval-side call
/// site never exists: a compiled method reading the trace sees the caller's callers and nothing
/// for the call that reached it.
fn eval_push_native_call_frame(
    class_name: &str,
    method_name: &str,
    object: Option<RuntimeCellHandle>,
    args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
) -> bool {
    if !crate::context::native_call_frames_enabled() {
        return false;
    }
    let frame = EvalCallFrame::method(class_name, method_name, object, Some(args.to_vec()), context);
    context.push_call_frame(frame);
    true
}

/// Calls one generated/AOT instance method from already evaluated positional values.
///
/// Internal eval operations still have to pass through the native argument binder. The binder
/// materializes compiler-internal physical slots, including the hidden argument collector, while
/// keeping the supplied runtime values borrowed from their caller.
pub(in crate::interpreter) fn eval_native_method_with_positional_values(
    object: RuntimeCellHandle,
    method_name: &str,
    values_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let class_name = eval_runtime_object_class_name(object, values)?;
    eval_native_method_with_evaluated_args(
        object,
        &class_name,
        method_name,
        positional_native_method_args(values_args),
        context,
        values,
    )
}

/// Calls one generated/AOT method from positional values and a prevalidated bridge scope.
pub(in crate::interpreter) fn eval_native_method_with_positional_values_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    values_args: Vec<RuntimeCellHandle>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        positional_native_method_args(values_args),
        bridge_scope,
        called_class_scope,
        context,
        values,
    )
}

/// Wraps already evaluated positional values for native signature binding.
fn positional_native_method_args(values: Vec<RuntimeCellHandle>) -> Vec<EvaluatedCallArg> {
    values
        .into_iter()
        .map(|value| EvaluatedCallArg {
            name: None,
            value,
            ref_target: None,
        })
        .collect()
}

/// Calls one generated/AOT instance method after native signature binding.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method after validation with an optional bridge scope.
pub(super) fn eval_native_method_with_evaluated_args_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    if resolved_bridge_scope.is_none() {
        if let Some(shadow_scope) =
            eval_private_scope_shadow_bridge_scope(class_name, method_name, context, values)?
        {
            // The calling scope's own private method shadows any override on
            // the receiver's class; access is inherently allowed, so skip the
            // hierarchy resolution (it would find the override instead).
            return eval_native_method_with_evaluated_args_unchecked_bridge_scope(
                object,
                class_name,
                method_name,
                evaluated_args,
                Some(&shadow_scope),
                called_class_scope,
                context,
                values,
            );
        }
    }
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, _, true, false)) = metadata.as_ref() {
        // php calls a STATIC method through an instance as a static call on the object's class:
        // `$request->enableHttpMethodParameterOverride()`. The instance bridge has no entry for
        // a static method, so the call fatally missed. `static::` inside it is the object's
        // class, which is what the called-class scope carries.
        return eval_native_static_method_with_evaluated_args_bridge_scope(
            class_name,
            method_name,
            evaluated_args,
            Some(bridge_scope.unwrap_or(declaring_class)),
            Some(called_class_scope.unwrap_or(class_name)),
            context,
            values,
        );
    }
    if let Some((declaring_class, visibility, _, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if !is_abstract
            && validate_eval_method_access(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            )
            .is_err()
        {
            if eval_native_instance_magic_method_available(class_name, context, values)? {
                return eval_native_magic_instance_method_call(
                    object,
                    class_name,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_instance_magic_method_available(class_name, context, values)? {
        return eval_native_magic_instance_method_call(
            object,
            class_name,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without enforcing member visibility.
pub(super) fn eval_native_method_with_evaluated_args_unchecked(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope(
        object,
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT instance method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        object,
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Whether `object`'s class has no method of this name at all, asked the way `method_exists()` asks.
///
/// A failed question counts as "not absent", so only a method php would also call undefined
/// turns the helper's bare failure into an `Error`.
fn eval_native_method_is_absent(
    object: RuntimeCellHandle,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> bool {
    let Ok(name) = values.string(method_name) else {
        return false;
    };
    let exists = crate::interpreter::eval_member_exists_result(
        "method_exists",
        &[object, name],
        context,
        values,
    );
    let _ = values.release(name);
    let Ok(exists) = exists else {
        return false;
    };
    let absent = values.truthy(exists).is_ok_and(|exists| !exists);
    let _ = values.release(exists);
    absent
}

/// Calls one generated/AOT instance method with a selected by-reference binding mode.
pub(super) fn eval_native_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = number_scalar_binding_signature(signature_owner,
        context.native_method_signature(signature_owner, method_name));
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    let bound_args = bind_native_callable_bound_args_with_mode(
        signature,
        evaluated_args,
        by_ref_mode,
        context,
        values,
    )
    .map_err(|status| {
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=native_method_error stage=bind class={class_name:?} method={method_name:?} status={status:?} class_scope={:?}",
                context.current_class_scope(),
            );
        }
        status
    })?;
    let mut result = RuntimeCellHandle::from_raw(std::ptr::null_mut());
    let arg_values = native_bound_arg_values(&bound_args);
    let pushed_frame = eval_push_native_call_frame(
        signature_owner,
        method_name,
        Some(object),
        &arg_values,
        context,
    );
    let call_result = context.with_eval_global_sync(|context| {
        if let Some(scope) = bridge_scope {
            eval_native_method_call_with_scope_out(
                scope,
                called_class_scope,
                object,
                method_name,
                arg_values,
                &mut result,
                context,
                values,
            )
        } else {
            values.method_call_out(object, method_name, arg_values, &mut result)
        }
    });
    if pushed_frame {
        context.pop_call_frame();
    }
    let writeback = if native_bound_args_require_writeback(&bound_args) {
        write_back_native_callable_ref_args(&bound_args, context, values)
    } else {
        Ok(())
    };
    let result = match (call_result, writeback) {
        (Err(status), _) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=native_method_error stage=invoke class={class_name:?} method={method_name:?} status={status:?} class_scope={:?}",
                    context.current_class_scope(),
                );
            }
            // The generated method helper answers a name the object's class lacks with a bare
            // failure and no pending throwable, which surfaced as an uncatchable fatal. php
            // raises a catchable `Error` there, and `try { $o->nope(); } catch (Error $e)` in
            // interpreted code depends on it (Laravel's console probes models this way).
            if status == EvalStatus::RuntimeFatal
                && eval_native_method_is_absent(object, method_name, context, values)
            {
                eval_throw_undefined_method_call_error(class_name, method_name, context, values)
            } else {
                Err(status)
            }
        }
        (Ok(()), Err(status)) => {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=native_method_error stage=writeback class={class_name:?} method={method_name:?} status={status:?} class_scope={:?}",
                    context.current_class_scope(),
                );
            }
            let _ = release_expr_result(result, context, values);
            Err(status)
        }
        (Ok(()), Ok(())) => {
            if crate::eval_trace::enabled() {
                let result_ptr = result.as_ptr();
                // This block used to read three words straight through the handle and aborted the
                // whole process -- a non-unwinding panic, SIGABRT with no backtrace and no
                // `test result:` line, so it read as an infrastructure failure rather than as one
                // bad read. It fired only with the trace ON.
                //
                // The handle is not always a cell pointer. `RuntimeValueOps` has two
                // implementations, and the test harness mints handles from indices, so its
                // `as_ptr()` yields values like `0x1`: not null, not aligned, not readable. Only
                // the implementation that owns the representation may look, so the peek is a trait
                // method whose default is `None`.
                let words = values.trace_result_words(result);
                let tag = values.type_tag(result);
                let object_refs = matches!(tag, Ok(EVAL_TAG_OBJECT)).then(|| {
                    let object_ptr = words.map_or(std::ptr::null(), |w| w[1] as *const u8);
                    // Kept as a braceless closure body on purpose: wrapping it in `{ }` makes
                    // `unsafe { ... }` a STATEMENT, and the `&` that follows is then parsed as a
                    // reference rather than a bitwise and -- `expected (), found u32`.
                    (!object_ptr.is_null())
                        .then(|| unsafe { object_ptr.sub(12).cast::<u32>().read_unaligned() } & 0x7fff_ffff)
                });
                let words_text = words.map_or_else(
                    || "null".to_string(),
                    |w| format!("[{:#x}, {:#x}, {:#x}]", w[0], w[1], w[2]),
                );
                eprintln!(
                    "[elephc-eval-trace] phase=native_method_return_contract owner={signature_owner:?} class={class_name:?} method={method_name:?} result={result_ptr:p} words={words_text} return_type={return_type:?} tag={tag:?} object_refs={object_refs:?}",
                );
            }
            eval_declared_native_return_value(
                return_type.as_ref(),
                Some(signature_owner),
                called_class_scope.or(Some(class_name)),
                result,
                context,
                values,
            )
            .map_err(|status| {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=native_method_error stage=return_value class={class_name:?} method={method_name:?} status={status:?} class_scope={:?}",
                        context.current_class_scope(),
                    );
                }
                status
            })
        }
    };
    finish_native_bound_call(result, &bound_args, context, values)
}

/// Calls one generated/AOT static method after native signature binding.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        None,
        None,
        context,
        values,
    )
}

/// Calls one generated/AOT static method after validation with an optional bridge scope.
pub(super) fn eval_native_static_method_with_evaluated_args_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut resolved_bridge_scope = bridge_scope.map(str::to_string);
    let metadata =
        eval_aot_method_dispatch_metadata_in_hierarchy(class_name, method_name, context, values)?;
    if let Some((declaring_class, visibility, is_static, is_abstract)) = metadata {
        if resolved_bridge_scope.is_none() {
            resolved_bridge_scope = Some(declaring_class.clone());
        }
        if is_static
            && !is_abstract
            && validate_eval_method_access(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            )
            .is_err()
        {
            if eval_native_static_magic_method_available(class_name, context, values)? {
                return eval_native_magic_static_method_call_as(
                    class_name,
                    called_class_scope,
                    method_name,
                    evaluated_args,
                    context,
                    values,
                );
            }
            return eval_throw_method_access_error(
                &declaring_class,
                method_name,
                visibility,
                context,
                values,
            );
        }
    } else if eval_native_static_magic_method_available(class_name, context, values)? {
        return eval_native_magic_static_method_call_as(
            class_name,
            called_class_scope,
            method_name,
            evaluated_args,
            context,
            values,
        );
    }
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        method_name,
        evaluated_args,
        resolved_bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}

/// Calls one generated/AOT static method without visibility checks using an optional bridge scope.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::RequireTarget,
        context,
        values,
    )
}

/// Calls one generated/AOT static method for `call_user_func()` by-value by-ref degradation.
pub(in crate::interpreter) fn eval_native_static_method_with_evaluated_args_for_call_user_func_unchecked_bridge_scope(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let callable_name = format!("{}::{}", signature_owner.trim_start_matches('\\'), method_name);
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
        class_name,
        method_name,
        evaluated_args,
        bridge_scope,
        called_class_scope,
        EvalByRefBindingMode::WarnByValue {
            callable_name: &callable_name,
        },
        context,
        values,
    )
}

/// Calls one generated/AOT static method with a selected by-reference binding mode.
pub(super) fn eval_native_static_method_with_evaluated_args_unchecked_bridge_scope_with_ref_mode(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    bridge_scope: Option<&str>,
    called_class_scope: Option<&str>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // `Fiber::getCurrent()` is a compiler intrinsic, not a method body the bridge can call:
    // Monolog's `Logger::addRecord()` asks it on every log call, and interpreted it fataled.
    if evaluated_args.is_empty()
        && class_name.trim_start_matches('\\').eq_ignore_ascii_case("Fiber")
        && method_name.eq_ignore_ascii_case("getCurrent")
    {
        return values.fiber_current();
    }
    let signature_owner = bridge_scope.unwrap_or(class_name);
    let signature = tokenizer_native_binding_signature(signature_owner, method_name,
        context.native_static_method_signature(signature_owner, method_name));
    let return_type = signature.as_ref().and_then(|signature| signature.return_type().cloned());
    // Three things can refuse below and every one of them meant the same anonymous fatal: no
    // signature recorded for the owner, a binding that rejects the arguments, and the generated
    // static-call helper itself.
    let traced = crate::eval_trace::enabled();
    if traced {
        eprintln!(
            "[elephc-eval-trace] phase=native_static_call class={class_name:?} method={method_name:?} owner={signature_owner:?} signature={} args={}",
            signature.is_some(),
            evaluated_args.len(),
        );
    }
    let mut bound_args = match bind_native_callable_bound_args_with_mode(
        signature,
        evaluated_args,
        by_ref_mode,
        context,
        values,
    ) {
        Ok(bound_args) => bound_args,
        Err(status) => {
            if traced {
                eprintln!(
                    "[elephc-eval-trace] phase=native_static_call stage=bind class={class_name:?} method={method_name:?} status={status:?}"
                );
            }
            return Err(status);
        }
    };
    let result = (|| {
        normalize_tokenizer_native_arguments(signature_owner, method_name, &mut bound_args, context, values)?;
        if let Some(result) = eval_tokenizer_static_method(signature_owner, method_name,
            called_class_scope.unwrap_or(class_name), &native_bound_arg_values(&bound_args),
            context, values) {
            return result;
        }
        let arg_values = native_bound_arg_values(&bound_args);
        let pushed_frame =
            eval_push_native_call_frame(signature_owner, method_name, None, &arg_values, context);
        let result = if let Some(scope) = bridge_scope {
            eval_native_static_method_call_with_scope(
                scope,
                called_class_scope,
                class_name,
                method_name,
                arg_values,
                context,
                values,
            )
        } else {
            values.static_method_call(class_name, method_name, arg_values)
        };
        if pushed_frame {
            context.pop_call_frame();
        }
        result
    })();
    if traced {
        eprintln!(
            "[elephc-eval-trace] phase=native_static_call stage=called class={class_name:?} method={method_name:?} outcome={}",
            match &result {
                Ok(_) => "ok".to_string(),
                Err(status) => format!("{status:?}"),
            },
        );
    }
    let writeback = if native_bound_args_require_writeback(&bound_args) {
        write_back_native_callable_ref_args(&bound_args, context, values)
    } else {
        Ok(())
    };
    let result = match (result, writeback) {
        (Ok(result), Err(status)) => {
            let _ = release_expr_result(result, context, values);
            Err(status)
        }
        (Err(status), _) => Err(status),
        (Ok(result), Ok(())) => eval_declared_native_return_value(
            return_type.as_ref(),
            Some(signature_owner),
            called_class_scope.or(Some(class_name)),
            result,
            context,
            values,
        ),
    };
    finish_native_bound_call(result, &bound_args, context, values)
}

/// Returns whether a generated/AOT class has an instance `__call()` fallback.
pub(super) fn eval_native_instance_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(eval_aot_method_dispatch_metadata_in_hierarchy(class_name, "__call", context, values)?
        .is_some_and(|(_, _, is_static, is_abstract)| !is_static && !is_abstract))
}

/// Returns whether a generated/AOT class has a static `__callStatic()` fallback.
pub(super) fn eval_native_static_magic_method_available(
    class_name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(
        eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .is_some_and(|(_, _, is_static, is_abstract)| is_static && !is_abstract),
    )
}

/// Dispatches a missing or inaccessible generated/AOT instance method through `__call()`.
pub(in crate::interpreter) fn eval_native_magic_instance_method_call(
    object: RuntimeCellHandle,
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    eval_native_method_with_evaluated_args_unchecked(
        object,
        class_name,
        "__call",
        magic_args,
        context,
        values,
    )
}

/// Dispatches a missing or inaccessible generated/AOT static method through `__callStatic()`.
pub(in crate::interpreter) fn eval_native_magic_static_method_call(
    class_name: &str,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_native_magic_static_method_call_as(
        class_name,
        None,
        method_name,
        evaluated_args,
        context,
        values,
    )
}

/// Dispatches through a generated/AOT `__callStatic()` with an explicit late-static-binding class.
///
/// `View::replaceNamespace(...)` on an interpreted facade whose compiled base `Facade` answers
/// through `__callStatic()` has to run that body with `static` = `View`: the body asks
/// `static::getFacadeAccessor()`, and without the called class it asked `Facade`'s, which throws
/// "Facade does not implement getFacadeAccessor method".
pub(in crate::interpreter) fn eval_native_magic_static_method_call_as(
    class_name: &str,
    called_class_scope: Option<&str>,
    method_name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let magic_args = eval_magic_call_args(method_name, evaluated_args, values)?;
    // An INTERPRETED called class only reaches the compiled body through a bridge scope (the
    // declaring class the magic method is looked up on). A compiled called class is already the
    // class the generated call names, and carries its own late-static-binding id.
    let bridge_scope = match called_class_scope {
        Some(called) if context.has_class(called) => eval_aot_method_dispatch_metadata_in_hierarchy(
            class_name,
            "__callStatic",
            context,
            values,
        )?
        .map(|(declaring_class, _, _, _)| declaring_class),
        _ => None,
    };
    let called_class_scope = bridge_scope.as_ref().and(called_class_scope);
    eval_native_static_method_with_evaluated_args_unchecked_bridge_scope(
        class_name,
        "__callStatic",
        magic_args,
        bridge_scope.as_deref(),
        called_class_scope,
        context,
        values,
    )
}
