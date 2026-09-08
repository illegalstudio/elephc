//! Purpose:
//! Owns the process-local registry that lets native object release call back
//! into eval-declared `__destruct()` methods for dynamic classes.
//!
//! Called from:
//! - `crate::context::ElephcEvalContext` when dynamic objects are registered or freed.
//! - The generated runtime through the installed destructor hook function pointer.
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
use crate::interpreter::RuntimeValueOps;
#[cfg(not(test))]
use crate::runtime_hooks::{self, ElephcRuntimeOps};
#[cfg(not(test))]
use crate::value::RuntimeCell;
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

/// Installs the eval dynamic object destructor callback into the generated runtime.
#[cfg(not(test))]
pub(crate) fn install_dynamic_object_destructor_hook() {
    unsafe {
        runtime_hooks::install_dynamic_object_destructor_hook(
            __elephc_eval_dynamic_object_destruct as *const () as usize,
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
    let escaped = if matches!(destruct_result, Err(EvalStatus::UncaughtThrowable)) {
        let thrown = context.take_pending_throw().ok_or(EvalStatus::RuntimeFatal)?;
        Some(if thrown.is_borrowed() { values.retain(thrown)? } else { thrown })
    } else {
        None
    };
    if let Some(previous) = previous_throw {
        context.set_pending_throw(previous);
    }
    values.release(object_cell)?;
    // The collector can still retain or resurrect the receiver. Final runtime
    // release, not destructor execution, retires its class and property metadata.
    if let Some(thrown) = escaped {
        unsafe { *throwable_out = thrown.as_ptr(); }
        return Ok(2);
    }
    destruct_result.map(u64::from)
}
