//! Purpose:
//! Owns the process-local registry that lets native object release call back
//! into eval-declared `__destruct()` methods for dynamic classes, and the sibling
//! callback that lets a generated `clone` reach the same eval class metadata.
//!
//! Called from:
//! - `crate::context::ElephcEvalContext` when dynamic objects are registered or freed.
//! - The generated runtime through the installed destructor and clone hook function pointers.
//!
//! Key details:
//! - The runtime owns object storage and calls this hook while the object is still
//!   intact, either in a pinned collector pass or in the final-release path.
//! - Registry values are stored as integer addresses so the global mutex remains
//!   `Sync`; every use revalidates null pointers and ABI version.

#[cfg(not(test))]
use crate::abi::ABI_VERSION;
use crate::abi::ElephcEvalContext;
#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::interpreter::eval_dynamic_destructor_for_object_cell;
#[cfg(not(test))]
use crate::interpreter::eval_object_clone_with_properties_for_ffi;
#[cfg(not(test))]
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::runtime_hooks::{self, ElephcRuntimeOps};
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static DYNAMIC_DESTRUCTOR_CONTEXTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();

/// Returns the process-local dynamic object to eval context registry.
fn dynamic_destructor_contexts() -> &'static Mutex<HashMap<u64, usize>> {
    DYNAMIC_DESTRUCTOR_CONTEXTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reports whether a raw object identity belongs to a live eval-declared class.
/// Native property helpers use this only after ordinary declared-property dispatch misses.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn __elephc_eval_dynamic_object_owns_properties(identity: u64) -> u64 {
    let Some(context) = dynamic_object_owner_context(identity) else { return 0; };
    // Closure identities also use this registry for metadata cleanup, but their
    // stdClass payload does not have eval-declared property storage.
    let Some(context) = (unsafe { context.as_ref() }) else { return 0; };
    u64::from(context.abi_version() == ABI_VERSION && context.dynamic_object_class(identity).is_some())
}

/// Installs the eval dynamic object destructor and clone callbacks into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_dynamic_object_destructor_hook() {
    unsafe {
        runtime_hooks::install_dynamic_object_destructor_hook(
            __elephc_eval_dynamic_object_destruct as *const () as usize,
        );
        runtime_hooks::install_dynamic_object_clone_hook(
            __elephc_eval_dynamic_object_clone as *const () as usize,
        );
    }
}

/// Records which eval context owns one dynamic object's eval class metadata.
pub(crate) fn register_dynamic_object_context(identity: u64, context: *mut ElephcEvalContext) {
    if identity == 0 || context.is_null() {
        return;
    }
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        contexts.insert(identity, context as usize);
    }
}

/// Removes one dynamic object from the process-local destructor registry.
pub(crate) fn unregister_dynamic_object(identity: u64) {
    if identity == 0 {
        return;
    }
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        contexts.remove(&identity);
    }
}

/// Removes every dynamic object currently associated with a soon-to-be-freed context.
pub(crate) fn unregister_dynamic_objects_for_context(context: *mut ElephcEvalContext) {
    if context.is_null() {
        return;
    }
    let context = context as usize;
    if let Ok(mut contexts) = dynamic_destructor_contexts().lock() {
        contexts.retain(|_, owner| *owner != context);
    }
}

/// Looks up the eval context that owns one dynamic object identity.
#[cfg(not(test))]
pub(crate) fn dynamic_object_owner_context(identity: u64) -> Option<*mut ElephcEvalContext> {
    let contexts = dynamic_destructor_contexts().lock().ok()?;
    let context = *contexts.get(&identity)?;
    Some(context as *mut ElephcEvalContext)
}

/// Drops final object metadata after receiver release, preserving it during collector destructors.
#[cfg(not(test))]
pub(crate) fn forget_released_object(identity: u64) {
    let Some(context) = dynamic_object_owner_context(identity) else { return; };
    // Context teardown unregisters its identities before freeing the context.
    let Some(context) = (unsafe { context.as_mut() }) else { return; };
    if context.abi_version() == ABI_VERSION {
        context.forget_dynamic_object(identity);
    }
}

/// Lifts an escaping eval Throwable out of a bridge result, retaining a borrowed one.
///
/// Split out so the cleanup below can run even when taking or retaining the Throwable fails.
/// Returning `Err` from inside the `escaped` expression itself used to skip both the pending-throw
/// restore and the receiver release.
#[cfg(not(test))]
fn take_escaping_throwable<T>(
    result: &Result<T, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut ElephcRuntimeOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if !matches!(result, Err(EvalStatus::UncaughtThrowable)) {
        return Ok(None);
    }
    let thrown = context.take_pending_throw().ok_or(EvalStatus::RuntimeFatal)?;
    Ok(Some(if thrown.is_borrowed() { values.retain(thrown)? } else { thrown }))
}

/// Restores the caller's pending throw and retires the bridge's own receiver box, unconditionally.
///
/// Both steps run before either failure is propagated, so no bridge exit path can drop the
/// caller's pending throw or leak the boxed `$this` the bridge made for the operation.
#[cfg(not(test))]
fn finish_bridge_ownership(
    escaped: Result<Option<RuntimeCellHandle>, EvalStatus>,
    previous_throw: Option<RuntimeCellHandle>,
    object_cell: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut ElephcRuntimeOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(previous) = previous_throw {
        context.set_pending_throw(previous);
    }
    let released = values.release(object_cell);
    let escaped = escaped?;
    released?;
    Ok(escaped)
}

/// Runs an eval destructor, returning zero for a miss, one for success, or two with an owned Throwable.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer. The runtime
/// calls this only while its object destruction guard bit is set, so boxing the
/// borrowed object for `$this` cannot recursively free the same storage.
/// `throwable_out` must point to a writable cell-pointer slot. Throws are transferred
/// through that output only after Rust has returned; native unwinding must not cross Rust frames.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_destruct(
    object: *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> u64 {
    match std::panic::catch_unwind(|| unsafe { dynamic_object_destruct_inner(object, throwable_out) }) {
        Ok(Ok(status)) => status,
        Ok(Err(_)) | Err(_) => {
            let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
            let _ = values.fatal("Fatal error: eval() destructor failed\n");
            std::process::abort()
        }
    }
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_destruct`; callers must pass a live raw
/// object pointer whose refcount guard marks destruction active and a writable output slot.
#[cfg(not(test))]
unsafe fn dynamic_object_destruct_inner(
    object: *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> Result<u64, EvalStatus> {
    if throwable_out.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe { *throwable_out = std::ptr::null_mut(); }
    if object.is_null() {
        return Ok(0);
    }
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return Ok(0);
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        unregister_dynamic_object(identity);
        return Ok(0);
    };
    if context.abi_version() != ABI_VERSION {
        unregister_dynamic_object(identity);
        return Ok(0);
    }
    if context.dynamic_object_class(identity).is_none() {
        // Closure metadata may own a foreign eval context needed by its receiver's
        // destructor. Its final owner callback drops metadata after child release.
        if context.closure_object_target(identity).is_none() {
            unregister_dynamic_object(identity);
        }
        return Ok(0);
    }

    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let object_cell = ElephcRuntimeOps::object_from_raw(object)?;
    let previous_throw = context.take_pending_throw();
    let destruct_result =
        eval_dynamic_destructor_for_object_cell(identity, object_cell, context, &mut values);
    let escaped = take_escaping_throwable(&destruct_result, context, &mut values);
    let escaped =
        finish_bridge_ownership(escaped, previous_throw, object_cell, context, &mut values)?;
    // The collector can still retain or resurrect the receiver. Final runtime
    // release, not destructor execution, retires its class and property metadata.
    if let Some(thrown) = escaped {
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    destruct_result.map(u64::from)
}

/// Clones an eval-owned object, returning zero for a miss, one for success, or two with an owned Throwable.
///
/// The generated `clone` lowering calls this before its own shallow-clone adapter. A zero
/// status means Magician does not own the identity, so the AOT path continues untouched and
/// ordinary generated objects, stdClass, and enums never change behavior.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer that the caller keeps
/// alive for the whole call; it stays borrowed. `properties` must be null or a borrowed
/// boxed PHP array cell. `scope_ptr` must be readable for `scope_len` bytes when
/// `scope_len > 0` and must hold the caller's lexical class name in UTF-8. `clone_out` and
/// `throwable_out` must point at writable cell-pointer slots; both receive owned boxes.
/// Throws are transferred through `throwable_out` only after Rust has returned, so native
/// unwinding never crosses a Rust frame.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_clone(
    object: *mut RuntimeCell,
    properties: *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
    clone_out: *mut *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> u64 {
    let cloned = std::panic::catch_unwind(|| unsafe {
        dynamic_object_clone_inner(object, properties, scope_ptr, scope_len, clone_out, throwable_out)
    });
    match cloned {
        Ok(Ok(status)) => status,
        Ok(Err(_)) | Err(_) => {
            let mut values = ElephcRuntimeOps::with_context(std::ptr::null());
            let _ = values.fatal("Fatal error: eval() clone failed\n");
            std::process::abort()
        }
    }
}

/// Executes the clone callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_clone`; callers must pass a live borrowed raw
/// object pointer, a borrowed or null override array box, readable scope bytes, and two
/// writable output slots.
#[cfg(not(test))]
unsafe fn dynamic_object_clone_inner(
    object: *mut RuntimeCell,
    properties: *mut RuntimeCell,
    scope_ptr: *const u8,
    scope_len: u64,
    clone_out: *mut *mut RuntimeCell,
    throwable_out: *mut *mut RuntimeCell,
) -> Result<u64, EvalStatus> {
    if clone_out.is_null() || throwable_out.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe {
        *clone_out = std::ptr::null_mut();
        *throwable_out = std::ptr::null_mut();
    }
    if object.is_null() {
        return Ok(0);
    }
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return Ok(0);
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return Ok(0);
    };
    if context.abi_version() != ABI_VERSION {
        return Ok(0);
    }
    // Only an identity with live eval class metadata can be cloned here. Closure payloads and
    // generated objects that merely pass through this registry stay on the AOT clone path.
    if context.dynamic_object_class(identity).is_none() {
        return Ok(0);
    }
    let scope = unsafe { aot_invocation_scope(scope_ptr, scope_len) };

    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    // The borrowed raw object needs one boxed owner for the shared clone operation; it is
    // retired below so the caller's own reference is the only one that survives the call.
    let object_cell = ElephcRuntimeOps::object_from_raw(object)?;
    // The override array stays owned by the caller for the whole call.
    let overrides =
        (!properties.is_null()).then(|| RuntimeCellHandle::from_raw(properties).borrowed());
    let previous_throw = context.take_pending_throw();
    // PHP checks `__clone()` visibility against the INVOCATION scope, so the caller's lexical
    // AOT class is pushed for the operation instead of letting it observe global scope.
    if let Some(scope) = scope.as_deref() {
        context.push_class_scope(scope);
    }
    let clone_result =
        eval_object_clone_with_properties_for_ffi(object_cell, overrides, context, &mut values);
    if scope.is_some() {
        context.pop_class_scope();
    }
    let escaped = take_escaping_throwable(&clone_result, context, &mut values);
    let escaped =
        finish_bridge_ownership(escaped, previous_throw, object_cell, context, &mut values)?;
    if let Some(thrown) = escaped {
        // The operation already released its own unfinished clone before unwinding, so the
        // Throwable box is the only owner handed back.
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    let clone = clone_result?;
    unsafe { *clone_out = clone.as_ptr(); }
    Ok(1)
}

/// Reads the caller's lexical AOT class name, treating an empty name as global scope.
///
/// # Safety
/// `scope_ptr` must be readable for `scope_len` bytes when `scope_len > 0`.
#[cfg(not(test))]
unsafe fn aot_invocation_scope(scope_ptr: *const u8, scope_len: u64) -> Option<String> {
    if scope_ptr.is_null() || scope_len == 0 {
        return None;
    }
    let scope_len = usize::try_from(scope_len).ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
    String::from_utf8(bytes.to_vec()).ok().filter(|scope| !scope.is_empty())
}
