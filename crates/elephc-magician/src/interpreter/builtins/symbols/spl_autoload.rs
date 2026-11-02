//! Purpose:
//! Eval registry entry and implementation for `spl_autoload`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Class resolution drives the registered callback table in PHP invocation order.

eval_builtin! {
    contract: "spl_autoload",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload(...)` calls against the registered callback table.
pub(in crate::interpreter) fn eval_spl_autoload_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_void("spl_autoload", args, context, scope, values)
}

/// Evaluates materialized `spl_autoload(...)` arguments against the registered callback table.
pub(in crate::interpreter) fn eval_spl_autoload_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_void_result("spl_autoload", evaluated_args, context, values)
}

/// Evaluates direct SPL autoload calls after validating their arity and source order.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_void(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (class, extensions) = match name {
        "spl_autoload_call" => match args {
            [class] => (class, None),
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "spl_autoload" => match args {
            [class] => (class, None),
            [class, extensions] => (class, Some(extensions)),
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let class = eval_expr(class, context, scope, values)?;
    if let Some(extensions) = extensions {
        let _ = eval_expr(extensions, context, scope, values)?;
    }
    eval_spl_autoload_void_result(name, &[class], context, values)
}

/// Evaluates materialized SPL autoload calls.
pub(in crate::interpreter) fn eval_spl_autoload_void_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_call" if evaluated_args.len() == 1 => {
            eval_spl_autoload_class_value(evaluated_args[0], context, values)?;
            values.null()
        }
        "spl_autoload" if (1..=2).contains(&evaluated_args.len()) => {
            eval_spl_autoload_class_value(evaluated_args[0], context, values)?;
            values.null()
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Invokes registered callbacks for one unresolved class and reports whether a class became visible.
///
/// PHP has ONE autoload queue per request and runs it in registration order. elephc stores each
/// callback on the context that registered it, so the queue is assembled here: every owner's
/// callbacks are merged and ordered by the request-wide number each was given at registration.
/// Running each context's list to the end before starting the next one gave the wrong order the
/// moment two contexts were involved -- Symfony appends `ClassExistenceResource::throwOnRequiredClass`
/// from one context and needs it to run AFTER Composer's loader, which lives in another.
pub(crate) fn eval_spl_autoload_class(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    if eval_autoload_target_exists(class_name, context, values)? {
        return Ok(true);
    }
    #[cfg(test)]
    {
        return eval_spl_autoload_class_local(class_name, context, values);
    }
    #[cfg(not(test))]
    {
        if !context.begin_autoload_class(class_name) {
            return Ok(false);
        }
        let skips_before = crate::interpreter::include_exec::compiler_include_skip_count();
        let result = eval_spl_autoload_queue(class_name, context, values);
        // The autoloader found the class's file, the compiler's mark skipped it, and the class is
        // still missing: the file's top-level code is what declares it (Carbon's `Translator.php`
        // `require`s the file that conditionally declares its parent, then declares the class),
        // and a compiled program only activates the file's classes. Run the file once more with
        // the mark ignored, the way the interface and trait paths already retry.
        let result = match result {
            Ok(false)
                if crate::interpreter::include_exec::compiler_include_skip_count() != skips_before
                    && !crate::interpreter::include_exec::reincluding_compiler_included_sources() =>
            {
                let retried =
                    crate::interpreter::include_exec::with_compiler_included_sources_reincluded(|| {
                        eval_spl_autoload_queue(class_name, context, values)
                    });
                context.sync_global_eval_classes();
                retried
            }
            other => other,
        };
        context.end_autoload_class(class_name);
        result
    }
}

/// Runs the request's whole autoload queue in registration order for one class name.
#[cfg(not(test))]
fn eval_spl_autoload_queue(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let here = context as *mut ElephcEvalContext;
    let mut owners = crate::context::global_eval_autoload_contexts_snapshot();
    if !owners.iter().any(|owner| std::ptr::eq(*owner, here)) {
        owners.push(here);
    }
    let mut queue: Vec<(i64, *mut ElephcEvalContext, RuntimeCellHandle)> = Vec::new();
    for owner in owners {
        let callbacks = if std::ptr::eq(owner, here) {
            context.autoload_callbacks_ordered()
        } else {
            match unsafe { owner.as_ref() } {
                Some(owner) => owner.autoload_callbacks_ordered(),
                None => continue,
            }
        };
        for (sequence, callback) in callbacks {
            queue.push((sequence, owner, callback));
        }
    }
    queue.sort_by_key(|(sequence, _, _)| *sequence);
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=spl_autoload_queue class={class_name:?} callbacks={}",
            queue.len(),
        );
    }
    for (_, owner, callback) in queue {
        if std::ptr::eq(owner, here) {
            eval_invoke_autoload_callback(callback, class_name, context, values)?;
        } else {
            let invoked = unsafe { eval_spl_autoload_callback_in_owner(owner, callback, class_name) };
            move_owner_pending_throw(&invoked, owner, context);
            invoked?;
            context.sync_global_eval_classes();
            let loaded_class = unsafe {
                owner
                    .as_ref()
                    .and_then(|owner| owner.class_shared(class_name).cloned())
            };
            if !context.has_class(class_name) {
                if let Some(loaded_class) = loaded_class {
                    context.define_class(loaded_class);
                }
            }
        }
        if eval_autoload_target_exists(class_name, context, values)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Moves a Throwable an autoload callback raised from its OWNER context to the probing caller.
///
/// A callback registered by compiled code runs on the owner's context, which is where its
/// escaping Throwable is parked, while the caller's `try` (or the eval result) consults its own.
/// Left there, `class_exists()` / `new Missing` surfaced a bare `UncaughtThrowable` status with
/// nothing to throw, and the exception vanished.
#[cfg(not(test))]
fn move_owner_pending_throw<T>(
    result: &Result<T, EvalStatus>,
    owner: *mut ElephcEvalContext,
    caller: &mut ElephcEvalContext,
) {
    if !matches!(result, Err(EvalStatus::UncaughtThrowable)) {
        return;
    }
    if let Some(thrown) = unsafe { owner.as_mut() }.and_then(|owner| owner.take_pending_throw()) {
        caller.set_pending_throw(thrown);
    }
}

/// Invokes ONE callback with runtime hooks bound to the context that registered it.
#[cfg(not(test))]
unsafe fn eval_spl_autoload_callback_in_owner(
    owner: *mut ElephcEvalContext,
    callback: RuntimeCellHandle,
    class_name: &str,
) -> Result<(), EvalStatus> {
    let Some(owner) = owner.as_mut() else {
        return Ok(());
    };
    crate::context::sync_global_eval_aot_metadata_when_empty(owner);
    let mut values = crate::runtime_hooks::ElephcRuntimeOps::with_context(owner as *const _);
    eval_invoke_autoload_callback(callback, class_name, owner, &mut values)
}

/// Normalizes one autoload callback, calls it with the class name, and releases what it made.
fn eval_invoke_autoload_callback(
    callback: RuntimeCellHandle,
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let class = values.string(class_name)?;
    let outcome = (|| {
        let callback = eval_callable(callback, context, values).map_err(|status| {
            if crate::eval_trace::enabled() {
                eprintln!("[elephc-eval-trace] phase=spl_autoload_callback stage=normalize status={status:?}");
            }
            status
        })?;
        let result = callback.with_values(context, values, |callback, context, values| {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=spl_autoload_callback stage=normalized kind={}",
                    eval_autoload_callback_trace_kind(callback),
                );
            }
            eval_evaluated_callable_with_values(callback, vec![class], context, values)
                .map_err(|status| {
                    if crate::eval_trace::enabled() {
                        eprintln!("[elephc-eval-trace] phase=spl_autoload_callback stage=invoke status={status:?}");
                    }
                    status
                })
        })?;
        eval_release_value(context, values, result)
    })();
    let release = eval_release_value(context, values, class);
    outcome?;
    release?;
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    Ok(())
}

/// Names one normalized callback for the opt-in autoload trace.
fn eval_autoload_callback_trace_kind(callback: &EvaluatedCallable) -> String {
    match callback {
        EvaluatedCallable::ForeignContext { callback, .. } => {
            format!("foreign:{}", eval_autoload_callback_trace_kind(callback))
        }
        EvaluatedCallable::Named { name, .. } => format!("named:{name}"),
        EvaluatedCallable::BoundClosure { name, .. } => format!("closure:{name}"),
        EvaluatedCallable::InvokableObject { .. } => "invokable-object".to_string(),
        EvaluatedCallable::NativeDescriptor { .. } => "native-descriptor".to_string(),
        EvaluatedCallable::ObjectMethod {
            method,
            native_class,
            ..
        } => format!("object-method:{native_class:?}::{method}"),
        EvaluatedCallable::StaticMethod {
            class_name,
            method,
            native_class,
            ..
        } => format!("static-method:{class_name}:{native_class:?}::{method}"),
    }
}

/// Loads a class-like declaration body needed by dynamic composition even when AOT metadata exists.
///
/// Generated metadata can report an AOT trait as existing while a dynamically included class still
/// needs the trait's EvalIR methods for `use Trait` composition. This path therefore checks only
/// context-owned declarations before invoking the ordinary SPL callback chain and synchronizing
/// the dynamic declaration registry.
pub(crate) fn eval_spl_autoload_classlike_definition(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = class_name.trim_start_matches('\\');
    #[cfg(not(test))]
    context.sync_global_eval_classes();
    if context.has_class(class_name)
        || context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
    {
        return Ok(true);
    }
    let _ = eval_spl_autoload_class_local(class_name, context, values)?;
    if context.has_class(class_name)
        || context.has_interface(class_name)
        || context.has_trait(class_name)
        || context.has_enum(class_name)
    {
        return Ok(true);
    }
    #[cfg(not(test))]
    for owner in crate::context::global_eval_autoload_contexts_snapshot() {
        if std::ptr::eq(owner, context) {
            continue;
        }
        let invoked = unsafe { eval_spl_autoload_class_in_owner(owner, class_name) };
        move_owner_pending_throw(&invoked, owner, context);
        let _ = invoked?;
        context.sync_global_eval_classes();
        if context.has_class(class_name)
            || context.has_interface(class_name)
            || context.has_trait(class_name)
            || context.has_enum(class_name)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Runs callbacks registered in exactly one context while preserving recursion protection.
fn eval_spl_autoload_class_local(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if !context.begin_autoload_class(class_name) {
        return Ok(false);
    }
    // The callbacks run in THIS context, which may be an owner the registration ABI allocated
    // before any metadata existed -- Composer's loader, registered from compiled code. Without a
    // native signature `loadClass($class)` was bound without its hidden `func_get_args` collector
    // and the generated method bridge refused it on arity: every request after the first died in
    // Twig's `new CoreExtension(...)`. The foreign-owner path below already took the snapshot.
    #[cfg(not(test))]
    crate::context::sync_global_eval_aot_metadata_when_empty(context);
    let class = values.string(class_name)?;
    let callbacks = context.autoload_callbacks();
    if crate::eval_trace::enabled() {
        eprintln!("[elephc-eval-trace] phase=spl_autoload_local class={class_name:?} callbacks={}", callbacks.len());
    }
    let result = (|| {
        for callback in callbacks {
            let callback = eval_callable(callback, context, values).map_err(|status| {
                if crate::eval_trace::enabled() {
                    eprintln!("[elephc-eval-trace] phase=spl_autoload_callback stage=normalize status={status:?}");
                }
                status
            })?;
            let result = callback.with_values(context, values, |callback, context, values| {
                if crate::eval_trace::enabled() {
                    let kind = eval_autoload_callback_trace_kind(callback);
                    eprintln!("[elephc-eval-trace] phase=spl_autoload_callback stage=normalized kind={kind}");
                }
                eval_evaluated_callable_with_values(callback, vec![class], context, values)
                    .map_err(|status| {
                        if crate::eval_trace::enabled() {
                            eprintln!("[elephc-eval-trace] phase=spl_autoload_callback stage=invoke status={status:?}");
                        }
                        status
                    })
            })?;
            eval_release_value(context, values, result).map_err(|status| {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=spl_autoload_callback stage=release status={status:?}",
                    );
                }
                status
            })?;
            #[cfg(not(test))]
            context.sync_global_eval_classes();
            if eval_autoload_target_exists(class_name, context, values)? {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=spl_autoload_callback stage=completed class={class_name:?}",
                    );
                }
                return Ok(true);
            }
        }
        Ok(false)
    })();
    let release = eval_release_value(context, values, class);
    context.end_autoload_class(class_name);
    release?;
    result
}

/// Returns whether an SPL autoload request made any PHP class-like symbol visible.
///
/// PHP uses the same callback registry for classes, interfaces, traits, and enums. A trait must
/// therefore count as loaded before a dynamically included class can expand its `use Trait` list.
fn eval_autoload_target_exists(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(context.has_class(name)
        || context.has_interface(name)
        || context.has_trait(name)
        || context.has_enum(name)
        || values.class_exists(name)?
        || eval_runtime_interface_exists(name, values)?
        || values.trait_exists(name)?
        || values.enum_exists(name)?)
}

/// Runs one foreign context's callbacks through runtime hooks bound to that owner.
#[cfg(not(test))]
unsafe fn eval_spl_autoload_class_in_owner(
    owner: *mut ElephcEvalContext,
    class_name: &str,
) -> Result<bool, EvalStatus> {
    let Some(owner) = owner.as_mut() else {
        return Ok(false);
    };
    // The owner of an AOT-registered callback is allocated by the registration ABI itself, before
    // the module has published its AOT metadata, so it knows no native signature and cannot supply
    // the default of a parameter the caller left out. This is the first moment that owner is used,
    // and the published snapshot is a module constant, so it can be taken now.
    crate::context::sync_global_eval_aot_metadata_when_empty(owner);
    let mut values = crate::runtime_hooks::ElephcRuntimeOps::with_context(owner as *const _);
    eval_spl_autoload_class_local(class_name, owner, &mut values)
}

/// Reads a PHP class-name argument and invokes registered callbacks for it.
fn eval_spl_autoload_class_value(
    class_name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = values.string_bytes(class_name)?;
    let class_name = String::from_utf8(class_name).map_err(|_| EvalStatus::RuntimeFatal)?;
    eval_spl_autoload_class(&class_name, context, values)
}
