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
//!   intact but already in the final-release path.
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

/// Runs an eval dynamic object destructor from the native object free path.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer. The runtime
/// calls this only while its object destruction guard bit is set, so boxing the
/// borrowed object for `$this` cannot recursively free the same storage.
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_dynamic_object_destruct(
    object: *mut RuntimeCell,
) -> u64 {
    std::panic::catch_unwind(|| unsafe { dynamic_object_destruct_inner(object) }).unwrap_or(0)
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_dynamic_object_destruct`; callers must pass a live raw
/// object pointer whose refcount guard already marks destruction as active.
#[cfg(not(test))]
unsafe fn dynamic_object_destruct_inner(object: *mut RuntimeCell) -> u64 {
    if object.is_null() {
        return 0;
    }
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return 0;
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        unregister_dynamic_object(identity);
        return 0;
    };
    if context.abi_version() != ABI_VERSION {
        unregister_dynamic_object(identity);
        return 0;
    }
    if context.dynamic_object_class(identity).is_none() {
        unregister_dynamic_object(identity);
        return 0;
    }

    let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
    let object_cell = match ElephcRuntimeOps::object_from_raw(object) {
        Ok(object_cell) => object_cell,
        Err(_) => {
            context.forget_dynamic_object(identity);
            return 1;
        }
    };
    let destruct_result =
        eval_dynamic_destructor_for_object_cell(identity, object_cell, context, &mut values);
    let release_result = values.release(object_cell);
    context.forget_dynamic_object(identity);
    match (destruct_result, release_result) {
        (Ok(true), Ok(())) | (Ok(false), Ok(())) => 1,
        (Err(EvalStatus::UnsupportedConstruct), _) => 1,
        (Err(_), _) | (_, Err(_)) => 1,
    }
}
