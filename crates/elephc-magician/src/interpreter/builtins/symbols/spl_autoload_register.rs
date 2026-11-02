//! Purpose:
//! Eval registry entry and implementation for `spl_autoload_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Callbacks retain their PHP values until unregistered or the web request resets.

eval_builtin! {
    contract: "spl_autoload_register",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_register(...)` calls and preserves source-order arguments.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (callback, throw, prepend) = match args {
        [callback] => (callback, None, None),
        [callback, throw] => (callback, Some(throw), None),
        [callback, throw, prepend] => (callback, Some(throw), Some(prepend)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let callback = eval_expr(callback, context, scope, values)?;
    if let Some(throw) = throw {
        let _ = eval_expr(throw, context, scope, values)?;
    }
    let prepend = prepend
        .map(|prepend| eval_expr(prepend, context, scope, values))
        .transpose()?
        .map(|prepend| values.truthy(prepend))
        .transpose()?
        .unwrap_or(false);
    register_spl_autoload_callback(callback, prepend, context, values)
}

/// Evaluates materialized `spl_autoload_register(...)` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (callback, throw, prepend) = match evaluated_args {
        [callback] => (*callback, None, None),
        [callback, throw] => (*callback, Some(*throw), None),
        [callback, throw, prepend] => (*callback, Some(*throw), Some(*prepend)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let _ = throw.map(|throw| values.truthy(throw)).transpose()?;
    let prepend = prepend
        .map(|prepend| values.truthy(prepend))
        .transpose()?
        .unwrap_or(false);
    register_spl_autoload_callback(callback, prepend, context, values)
}

/// Evaluates direct `spl_autoload_unregister(...)` while preserving its callback value.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_register" => eval_spl_autoload_register_declared_call(args, context, scope, values),
        "spl_autoload_unregister" => {
            let [callback] = args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let callback = eval_expr(callback, context, scope, values)?;
            eval_spl_autoload_unregister_result(callback, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates materialized SPL autoload registration calls.
pub(in crate::interpreter) fn eval_spl_autoload_bool_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_register" => {
            eval_spl_autoload_register_declared_values_result(evaluated_args, context, values)
        }
        "spl_autoload_unregister" => match evaluated_args {
            [callback] => eval_spl_autoload_unregister_result(*callback, context, values),
            _ => Err(EvalStatus::RuntimeFatal),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Validates and retains one callback in the request-global SPL autoload table.
pub(crate) fn register_spl_autoload_callback(
    callback: RuntimeCellHandle,
    prepend: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let normalized = eval_callable(callback, context, values)?;
    // PHP validates the callback AT REGISTRATION and throws `TypeError` naming what is wrong with
    // it, rather than waiting for the first autoload attempt. Deferring the check moves the
    // failure to a line the program never wrote, and hides it completely for a program that
    // happens never to autoload anything — which is most of them until the day it matters.
    normalized.with_check(context, values, |callable, context, values| {
        eval_validate_spl_autoload_register_callback(callable, context, values)
    })?;
    register_spl_autoload_callback_unchecked(callback, prepend, context, values)
}

/// Retains an AOT callback whose native descriptor is validated when the class is requested.
pub(crate) fn register_spl_autoload_callback_unchecked(
    callback: RuntimeCellHandle,
    prepend: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // PHP deduplicates by VALUE: registering the same loader twice reports success and still
    // leaves one registration.
    if eval_autoload_callback_position(callback, context, values)?.is_some() {
        return values.bool_value(true);
    }
    let first_callback = context.has_no_autoload_callbacks();
    if first_callback
        && !crate::context::register_global_eval_autoload_context(
            context as *mut ElephcEvalContext,
            prepend,
        )
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let callback = values.retain(callback)?;
    context.register_autoload_callback(callback, prepend);
    values.bool_value(true)
}

/// Removes one AOT-registered autoload callback from whichever eval context retains it.
///
/// The generated `spl_autoload_register` lowering registers through
/// `__elephc_eval_register_spl_autoload`, which creates its OWN request-scoped context when the
/// caller has none, so the matching unregister cannot be told which context holds the callback.
/// It asks every context that has autoload callbacks -- the same snapshot the autoload dispatch
/// itself walks.
///
/// Until this existed, the generated `spl_autoload_unregister` was a stub that evaluated its
/// argument and returned `true` without removing anything. MEASURED on php 8.5.10: a loader
/// registered, used, then unregistered is called ONCE; elephc called it for every later lookup
/// too. Symfony's `ClassExistenceResource` installs a THROWING loader and removes it in a
/// `finally`, so the leftover turned every subsequent `class_exists()` in the process into a
/// throw -- which is where `bin/console` died.
///
/// # Safety
/// The snapshot holds context pointers the bridge keeps live for the request; a pointer that no
/// longer resolves is skipped rather than dereferenced blindly.
#[cfg(not(test))]
pub(crate) fn unregister_runtime_spl_autoload_callback(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for owner in crate::context::global_eval_autoload_contexts_snapshot() {
        let Some(context) = (unsafe { owner.as_mut() }) else {
            continue;
        };
        let Some(index) = eval_autoload_callback_position(callback, context, values)? else {
            continue;
        };
        let Some(removed) = context.remove_autoload_callback_at(index) else {
            continue;
        };
        eval_release_value(context, values, removed)?;
        if context.has_no_autoload_callbacks() {
            crate::context::unregister_global_eval_autoload_context(owner);
        }
        return Ok(true);
    }
    Ok(false)
}

/// Reports whether ANY eval context already holds an autoload callback equal to this one.
///
/// The generated `spl_autoload_register` passes no context, so each call registers into a fresh
/// one and the per-context check in [`register_spl_autoload_callback_unchecked`] never saw the
/// earlier registration: registering one loader twice kept both, and php's single call became
/// two. The lookup walks the same contexts unregistration does.
#[cfg(not(test))]
pub(crate) fn runtime_spl_autoload_callback_is_registered(
    callback: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for owner in crate::context::global_eval_autoload_contexts_snapshot() {
        let Some(context) = (unsafe { owner.as_ref() }) else {
            continue;
        };
        if eval_autoload_callback_position(callback, context, values)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns the position of a registered autoload callback equal BY VALUE to this one.
///
/// PHP matches an autoload callback by value: the same `'name'` written at registration and at
/// unregistration is two different cells and one callback. Comparing cell identity made
/// unregistration answer `false` for a callback that was plainly there, and let the same loader
/// be registered twice — `php -n` 8.5.6 registers `'a_loader'` twice and still reports one.
fn eval_autoload_callback_position(
    callback: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<usize>, EvalStatus> {
    for (index, registered) in context.autoload_callbacks().into_iter().enumerate() {
        if registered.as_ptr() == callback.as_ptr() {
            return Ok(Some(index));
        }
        let equal = values.compare(EvalBinOp::StrictEq, registered, callback)?;
        let matched = values.truthy(equal);
        let _ = values.release(equal);
        if matched? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

/// Removes one retained callback and reports PHP's boolean unregister result.
fn eval_spl_autoload_unregister_result(
    callback: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // PHP distinguishes "this is not a callback" from "this callback is not registered": the
    // first is a TypeError, only the second is `false`.
    let normalized = eval_callable(callback, context, values)?;
    normalized.with_check(context, values, |callable, context, values| {
        eval_validate_spl_autoload_unregister_callback(callable, context, values)
    })?;
    let Some(index) = eval_autoload_callback_position(callback, context, values)? else {
        return values.bool_value(false);
    };
    let Some(callback) = context.remove_autoload_callback_at(index) else {
        return values.bool_value(false);
    };
    eval_release_value(context, values, callback)?;
    if context.has_no_autoload_callbacks() {
        crate::context::unregister_global_eval_autoload_context(context as *mut ElephcEvalContext);
    }
    values.bool_value(true)
}
