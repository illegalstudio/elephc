//! Purpose:
//! Exports native-to-eval callable dispatch and probes for callback values that
//! may reference eval-declared functions, methods, or objects. Generated code
//! uses this ABI when native descriptor metadata cannot answer dynamically.
//!
//! Called from:
//! - Generated EIR backend assembly through `__elephc_eval_callable_call_array`.
//! - Generated EIR backend assembly through `__elephc_eval_is_callable`.
//!
//! Key details:
//! - Callback and argument containers are boxed Mixed cells owned by generated
//!   code. Dispatch results and uncaught throwables are returned through
//!   `ElephcEvalResult`; probe failures fail closed as `false`.

use super::util::{clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::errors::EvalStatus;
use crate::interpreter::{self, RuntimeValueOps};
use crate::runtime_hooks::ElephcRuntimeOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

/// Checks whether a callback value is callable in the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle and `callback` must point at a
/// boxed runtime cell.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_is_callable(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_is_callable");
    std::panic::catch_unwind(|| unsafe { eval_is_callable_inner(ctx, callback) }).unwrap_or(0)
}

/// Returns the live eval context that owns a PHP Closure object, or null for every other value.
///
/// # Safety
/// `callback` must be null or a live boxed runtime cell. The lookup borrows only the object
/// identity; a context remains alive after its AOT frame returns while this Closure is live.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_callable_owner_context(
    callback: *mut RuntimeCell,
) -> *mut ElephcEvalContext {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_callable_owner_context");
    std::panic::catch_unwind(|| unsafe { eval_callable_owner_context_inner(callback) })
        .unwrap_or(std::ptr::null_mut())
}

/// Returns the eval context of the innermost interpreted call into a registered AOT function.
///
/// Null when no such call is running. Generated invokers use it to resolve an interpreted
/// callback handed to a `callable` parameter when the callback owns no context of its own.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn __elephc_eval_native_call_context() -> *mut ElephcEvalContext {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_native_call_context");
    std::panic::catch_unwind(interpreter::current_native_call_context)
        .unwrap_or(std::ptr::null_mut())
}

/// Returns the live eval context that declared a dynamic PHP function, or null when absent.
///
/// # Safety
/// `name_ptr` must be null or readable for `name_len` bytes. The context remains live for the
/// current request and is removed from this registry before a web worker recycles its heap.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_function_owner_context(
    name_ptr: *const u8,
    name_len: u64,
) -> *mut ElephcEvalContext {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_function_owner_context");
    std::panic::catch_unwind(|| unsafe { eval_function_owner_context_inner(name_ptr, name_len) })
        .unwrap_or(std::ptr::null_mut())
}

/// Registers an AOT callback in the persistent SPL autoload table of its eval context.
///
/// # Safety
/// `ctx` must be a valid bridge context and `callback` must be a live boxed runtime cell.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_register_spl_autoload(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    prepend: i32,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_register_spl_autoload");
    std::panic::catch_unwind(|| unsafe {
        let (context, created_context) = if let Some(context) = ctx.as_mut() {
            (context, false)
        } else {
            let context = crate::ffi::context::__elephc_eval_context_new();
            let Some(context) = context.as_mut() else {
                return 0;
            };
            (context, true)
        };
        if context.abi_version() != ABI_VERSION || callback.is_null() {
            return 0;
        }
        if crate::eval_trace::enabled() {
            eprintln!("[elephc-eval-trace] phase=aot_autoload_register stage=entered callback={callback:p} prepend={prepend}");
        }
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        let result = crate::interpreter::register_runtime_spl_autoload_callback(
            RuntimeCellHandle::from_raw(callback),
            prepend != 0,
            context,
            &mut values,
        );
        if crate::eval_trace::enabled() {
            eprintln!("[elephc-eval-trace] phase=aot_autoload_register stage=registered success={}", result.is_ok());
        }
        // The answer crosses as an int; the bool cell the registration built is ours to drop,
        // while the context it was made under is still alive.
        let answer = match result {
            Ok(cell) => {
                let _ = values.release(cell);
                1
            }
            Err(_) => 0,
        };
        if created_context {
            let context_ptr = context as *mut ElephcEvalContext;
            if context.request_retained_context_free() {
                crate::ffi::context::finalize_eval_context_free(context_ptr);
            }
        }
        answer
    })
    .unwrap_or(0)
}

/// Returns the request's registered SPL autoload callables as a boxed indexed array.
///
/// `ctx` mirrors [`__elephc_eval_register_spl_autoload`]: generated callers pass null so a
/// request-scoped context can be created just to reach the value operations, while interpreted
/// callers pass their own. The returned cell is OWNED by the caller; a null answer means the
/// interpreter could not build the array.
///
/// # Safety
/// `ctx` must be null or a valid bridge context. The returned pointer must be released exactly
/// once by the caller.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_spl_autoload_functions(
    ctx: *mut ElephcEvalContext,
) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_spl_autoload_functions");
    std::panic::catch_unwind(|| unsafe {
        let (context, created_context) = if let Some(context) = ctx.as_mut() {
            (context, false)
        } else {
            let context = crate::ffi::context::__elephc_eval_context_new();
            let Some(context) = context.as_mut() else {
                return std::ptr::null_mut();
            };
            (context, true)
        };
        if context.abi_version() != ABI_VERSION {
            return std::ptr::null_mut();
        }
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        let result = crate::interpreter::registered_spl_autoload_functions(context, &mut values);
        drop(values);
        let answer = match result {
            Ok(cell) => cell.as_ptr(),
            Err(status) => {
                if crate::eval_trace::enabled() {
                    eprintln!("[elephc-eval-trace] phase=aot_autoload_functions stage=failed status={status:?}");
                }
                std::ptr::null_mut()
            }
        };
        if created_context {
            let context_ptr = context as *mut ElephcEvalContext;
            if context.request_retained_context_free() {
                crate::ffi::context::finalize_eval_context_free(context_ptr);
            }
        }
        answer
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Removes an AOT callback from the persistent SPL autoload table that retains it.
///
/// Returns 1 when a callback was removed and 0 otherwise, which is PHP's
/// `spl_autoload_unregister` boolean. The generated lowering used to answer a hardcoded `true`
/// without removing anything, so a loader stayed installed for the life of the process. MEASURED
/// on php 8.5.10: a loader registered, used, then unregistered is called ONCE; elephc called it
/// for every later class lookup too. Symfony's `ClassExistenceResource` installs a THROWING loader
/// and removes it in a `finally`, so the leftover turned every subsequent `class_exists()` into a
/// throw -- which is where `bin/console` died.
///
/// `ctx` mirrors `__elephc_eval_register_spl_autoload` and only supplies value operations: a
/// registration from generated code may have created its own request-scoped context, so the
/// callback is searched for across every context that holds autoload callbacks.
///
/// # Safety
/// `ctx` must be null or a valid bridge context, and `callback` must be a live boxed runtime cell.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_unregister_spl_autoload(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_unregister_spl_autoload");
    std::panic::catch_unwind(|| unsafe {
        if callback.is_null() {
            return 0;
        }
        let (context, created_context) = if let Some(context) = ctx.as_mut() {
            (context, false)
        } else {
            let context = crate::ffi::context::__elephc_eval_context_new();
            let Some(context) = context.as_mut() else {
                return 0;
            };
            (context, true)
        };
        if context.abi_version() != ABI_VERSION {
            return 0;
        }
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        let removed = crate::interpreter::remove_runtime_spl_autoload_callback(
            RuntimeCellHandle::from_raw(callback),
            &mut values,
        );
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] phase=aot_autoload_unregister callback={callback:p} removed={removed:?}"
            );
        }
        if created_context {
            let context_ptr = context as *mut ElephcEvalContext;
            if context.request_retained_context_free() {
                crate::ffi::context::finalize_eval_context_free(context_ptr);
            }
        }
        removed.map(i32::from).unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Dispatches a callback value with a PHP argument array through the eval context.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `callback` and `arg_array` must
/// point at boxed runtime cells, and `out` may be null.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_callable_call_array(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_callable_call_array");
    std::panic::catch_unwind(|| unsafe {
        eval_callable_call_array_inner(ctx, callback, arg_array, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Rebinds `$this` on an eval `Closure` object for the generated `Closure::bind` runtime.
///
/// Returns one with an owned boxed `Closure` in `out`, or zero when the value is not an eval
/// closure or the binding failed. Installed into the generated runtime through
/// `__elephc_eval_install_closure_bind_hook_v1`, so a callback-adapter descriptor reaching
/// `__rt_closure_bind` can be rebound instead of aborting as an unsupported capture shape.
///
/// # Safety
/// `ctx` must be a valid eval context handle. `closure` must point at the boxed callback cell
/// the adapter descriptor captured; it stays borrowed. `new_this` must be a live raw elephc
/// object pointer. `out` must point at a writable cell-pointer slot.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_closure_bind_this(
    ctx: *mut ElephcEvalContext,
    closure: *mut RuntimeCell,
    new_this: *mut RuntimeCell,
    out: *mut *mut RuntimeCell,
) -> u64 {
    std::panic::catch_unwind(|| unsafe { eval_closure_bind_this_inner(ctx, closure, new_this, out) })
        .unwrap_or(0)
}

/// Runs the closure rebinding ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_closure_bind_this`.
#[cfg(not(test))]
unsafe fn eval_closure_bind_this_inner(
    ctx: *mut ElephcEvalContext,
    closure: *mut RuntimeCell,
    new_this: *mut RuntimeCell,
    out: *mut *mut RuntimeCell,
) -> u64 {
    if out.is_null() {
        return 0;
    }
    unsafe { *out = std::ptr::null_mut(); }
    let Some(context) = (unsafe { ctx.as_mut() }) else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || closure.is_null() || new_this.is_null() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let Ok(receiver) = ElephcRuntimeOps::object_from_raw(new_this) else {
        return 0;
    };
    let bound = interpreter::execute_context_closure_bind_this(
        context,
        RuntimeCellHandle::from_raw(closure).borrowed(),
        receiver,
        &mut values,
    );
    // The bound closure retains the receiver as its own child, so this bridge's boxed owner
    // must not outlive the call; keeping it pinned the receiver and the closure's metadata.
    let released = values.release(receiver);
    match (bound, released) {
        (Ok(bound), Ok(())) => {
            unsafe { *out = bound.as_ptr(); }
            1
        }
        _ => 0,
    }
}

/// Runs the eval callable-probe ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_is_callable`; invalid handles fail closed as false.
#[cfg(not(test))]
unsafe fn eval_is_callable_inner(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
) -> i32 {
    let Some(context) = ctx.as_mut() else {
        return 0;
    };
    if context.abi_version() != ABI_VERSION || callback.is_null() {
        return 0;
    }
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    match interpreter::execute_context_is_callable(
        context,
        RuntimeCellHandle::from_raw(callback).borrowed(),
        &mut values,
    ) {
        Ok(callable) => i32::from(callable),
        Err(_) => 0,
    }
}

/// Looks up a closure's owning context from its native object identity.
///
/// # Safety
/// Mirrors `__elephc_eval_callable_owner_context`; the caller owns the boxed callback cell for
/// the full duration of this lookup.
#[cfg(not(test))]
unsafe fn eval_callable_owner_context_inner(
    callback: *mut RuntimeCell,
) -> *mut ElephcEvalContext {
    if callback.is_null() {
        return std::ptr::null_mut();
    }
    let callback = RuntimeCellHandle::from_raw(callback);
    // Every eval-owned callable SHAPE, not just a Closure object. `[$object, 'method']` is a
    // callable in PHP exactly as much as a Closure is, and when the object's class was declared
    // at runtime the generated candidate tables cannot name it — the native lookup misses and
    // this is the only resolver left. Refusing an array here turned Symfony's
    // `EventDispatcher::optimizeListeners`, which builds `$listener(...)` out of
    // `[$service, 'onKernelRequest']`, into a process-killing "mixed value is not callable".
    //
    // Both callers re-validate what comes back with `__elephc_eval_is_callable`, so widening the
    // lookup cannot accept a value the interpreter would refuse to call.
    let Some(context) = eval_callable_owner_context(callback) else {
        return std::ptr::null_mut();
    };
    let Some(context_ref) = (unsafe { context.as_ref() }) else {
        return std::ptr::null_mut();
    };
    if context_ref.abi_version() != ABI_VERSION {
        return std::ptr::null_mut();
    }
    context
}

/// Resolves one PHP function name through the request-local dynamic declaration registry.
///
/// # Safety
/// Mirrors `__elephc_eval_function_owner_context`; the name slice remains borrowed only during
/// this lookup.
#[cfg(not(test))]
unsafe fn eval_function_owner_context_inner(
    name_ptr: *const u8,
    name_len: u64,
) -> *mut ElephcEvalContext {
    if name_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let Ok(name) = std::str::from_utf8(unsafe {
        std::slice::from_raw_parts(name_ptr, name_len as usize)
    }) else {
        return std::ptr::null_mut();
    };
    let Some(context) = crate::context::global_eval_function_owner_context(name) else {
        return std::ptr::null_mut();
    };
    let Some(context_ref) = (unsafe { context.as_ref() }) else {
        return std::ptr::null_mut();
    };
    (context_ref.abi_version() == ABI_VERSION).then_some(context).unwrap_or(std::ptr::null_mut())
}

/// Runs the eval callable-array ABI body after installing a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_callable_call_array`; callers must provide boxed callback and
/// argument-array cells. A null context requests a temporary request-global fallback.
#[cfg(not(test))]
unsafe fn eval_callable_call_array_inner(
    ctx: *mut ElephcEvalContext,
    callback: *mut RuntimeCell,
    arg_array: *mut RuntimeCell,
    out: *mut ElephcEvalResult,
) -> i32 {
    if callback.is_null() || arg_array.is_null() {
        return EvalStatus::RuntimeFatal.code();
    }
    let callback = RuntimeCellHandle::from_raw(callback);
    let caller_context = (!ctx.is_null()).then_some(ctx);
    let owner_context = eval_callable_owner_context(callback);
    let (context_ptr, created_context) = if let Some(context) = owner_context.or(caller_context) {
        (context, false)
    } else {
        let context = crate::ffi::context::__elephc_eval_context_new();
        if context.is_null() {
            return EvalStatus::RuntimeFatal.code();
        }
        (context, true)
    };
    let Some(context) = context_ptr.as_mut() else {
        return EvalStatus::RuntimeFatal.code();
    };
    if context.abi_version() != ABI_VERSION {
        if created_context {
            crate::ffi::context::finalize_eval_context_free(context_ptr);
        }
        return EvalStatus::AbiMismatch.code();
    }
    if created_context {
        crate::context::sync_global_eval_aot_metadata(context);
        context.sync_global_eval_classes();
    }
    let forwarded_scopes = caller_context
        .filter(|caller| !std::ptr::eq(*caller, context))
        .and_then(|caller| caller.as_ref())
        .map(|caller| {
            (
                caller.current_class_scope().map(str::to_string),
                caller.current_called_class_scope().map(str::to_string),
            )
        });
    if let Some((class_scope, called_class_scope)) = &forwarded_scopes {
        if let Some(class_scope) = class_scope {
            context.push_class_scope(class_scope.clone());
        }
        if let Some(called_class_scope) = called_class_scope {
            context.push_called_class_scope(called_class_scope.clone());
        }
    }
    // A compiled frame with no eval context publishes its lexical class instead of forwarding
    // scopes (`__elephc_eval_push_native_caller_class`), as `__elephc_eval_method_call` already
    // honours. Without it `call_user_func_array([$this, 'privateMethod'], ...)` from inside the
    // class was refused "cannot access private method".
    let native_caller_class = forwarded_scopes
        .is_none()
        .then(crate::context::current_native_caller_class)
        .flatten()
        .filter(|class_name| !class_name.is_empty());
    if let Some(class_name) = &native_caller_class {
        context.push_class_scope(class_name.clone());
    }
    // Every caller of this ABI is generated native code. Refresh the captured
    // scope before interpreting the callback, then publish its changes before
    // returning either a value or an error to the native caller.
    let global_sync = context.native_global_sync.zip(context.global_scope_ptr());
    if let Some((hooks, scope)) = global_sync {
        (hooks.native_to_eval)(scope);
    }
    clear_result(out);
    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let status = match interpreter::execute_context_callable_call_array_outcome(
        context,
        callback.borrowed(),
        RuntimeCellHandle::from_raw(arg_array).borrowed(),
        &mut values,
    ) {
        Ok(outcome) => write_outcome(outcome, out).code(),
        Err(status) => status.code(),
    };
    if let Some((class_scope, called_class_scope)) = forwarded_scopes {
        if called_class_scope.is_some() {
            context.pop_called_class_scope();
        }
        if class_scope.is_some() {
            context.pop_class_scope();
        }
    }
    if native_caller_class.is_some() {
        context.pop_class_scope();
    }
    if let Some((hooks, scope)) = global_sync {
        (hooks.eval_to_native)(scope);
    }
    if created_context && context.request_retained_context_free() {
        crate::ffi::context::finalize_eval_context_free(context_ptr);
    }
    status
}

/// Resolves the owner context of an eval-owned Closure or object-method callback array.
#[cfg(not(test))]
fn eval_callable_owner_context(
    callback: RuntimeCellHandle,
) -> Option<*mut ElephcEvalContext> {
    let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
    if let Ok(identity) = values.object_identity(callback) {
        if let Some(context) =
            crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)
        {
            return Some(context);
        }
        // Falls through ON PURPOSE. `object_identity` answers for a value that is not an object
        // too, and returning here on the strength of that answer skipped the array probe below
        // for every `[$object, 'method']` callable — the one shape this resolver exists for.
    }
    // Gated on the value ACTUALLY being an array. Reading an element of an object raises PHP's
    // `Cannot use object of type X as array`, so probing unconditionally turned an invokable
    // object handed to an AOT `callable` parameter into a fatal. `is_array_like` is the wrong
    // gate here: it answers for anything eval can index, objects included.
    const EVAL_TAG_ARRAY: u64 = 4;
    const EVAL_TAG_ASSOC: u64 = 5;
    if !matches!(
        values.type_tag(callback),
        Ok(EVAL_TAG_ARRAY) | Ok(EVAL_TAG_ASSOC)
    ) {
        return eval_declared_callable_name_context(callback, &mut values);
    }
    // The key and the element come back as OWNED cells. Dropping them without a release leaked
    // both -- and this probe runs for every `is_callable()` on an array callable in a program
    // that links the eval bridge, found or not.
    if let Ok(key) = values.array_iter_key(callback, 0) {
        let receiver = values.array_get(callback, key);
        let _ = values.release(key);
        if let Ok(receiver) = receiver {
            let context = values
                .object_identity(receiver)
                .ok()
                .and_then(crate::ffi::dynamic_destructors::dynamic_object_owner_context)
                .or_else(|| eval_declared_callable_name_context(receiver, &mut values));
            let _ = values.release(receiver);
            if context.is_some() {
                return context;
            }
        }
    }
    eval_declared_callable_name_context(callback, &mut values)
}

/// Resolves a callable named by a STRING — `'Svc::make'`, `'Svc'` in `['Svc', 'make']`, or a
/// dynamically declared function name — to the context that can dispatch it.
///
/// A runtime-declared class has no single owning context the way an object does: every eval
/// context mirrors the process-wide class registry. The shared process-lifetime context is that
/// mirror, so it is the right dispatcher, and it outlives any request-scoped one.
///
/// A name that belongs to no runtime declaration answers `None` on purpose, which leaves compiled
/// callables on their existing native path instead of routing them through the interpreter.
#[cfg(not(test))]
fn eval_declared_callable_name_context(
    value: RuntimeCellHandle,
    values: &mut ElephcRuntimeOps,
) -> Option<*mut ElephcEvalContext> {
    const EVAL_TAG_STRING: u64 = 1;
    if values.type_tag(value).ok()? != EVAL_TAG_STRING {
        return None;
    }
    let name = String::from_utf8(values.string_bytes(value).ok()?).ok()?;
    let class_like = name.split_once("::").map_or(name.as_str(), |(class, _)| class);
    if crate::context::global_eval_class_is_declared(class_like) {
        return Some(crate::ffi::context::shared_null_handle_context() as *mut ElephcEvalContext);
    }
    crate::context::global_eval_function_owner_context(&name)
}
