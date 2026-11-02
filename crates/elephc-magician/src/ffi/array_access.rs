//! Purpose:
//! Answers `$object[$key]` reads the generated runtime cannot, for objects the interpreter built.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in `_elephc_eval_array_access_fn`,
//!   when `__rt_mixed_array_get` finds no compiled `ArrayAccess::offsetGet` for the receiver.
//!
//! Key details:
//! - An object the INTERPRETER built carries `stdClass`'s runtime class id whatever class
//!   declared it, so the compiled dispatch table had no `offsetGet` for it and every compiled
//!   `$config['view']` over an eval-declared `ArrayAccess` class raised "Cannot use object of
//!   type stdClass as array". Laravel's config repository is such a class whenever it is loaded
//!   at run time.
//! - Answers 0 for an object no eval context owns, or one whose class is not `ArrayAccess`: the
//!   runtime then raises its own error, exactly as before.

use crate::ffi::dynamic_destructors::dynamic_object_owner_context;
use crate::interpreter::{self, EvalOutcome, RuntimeValueOps};
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use crate::value::RuntimeCell;
use std::ffi::c_void;

/// The object is not one this hook answers for.
const NOT_HANDLED: u64 = 0;
/// `offsetGet()` returned; the owned result is in `*out`.
const HANDLED: u64 = 1;
/// `offsetGet()` threw; the owned throwable is in `*out`.
const THREW: u64 = 2;

/// The runtime's normalized-key sentinel: an integer key carries `key_hi == -1`.
const INT_KEY_SENTINEL: u64 = u64::MAX;

/// Installs the eval `ArrayAccess` read callback into the generated runtime.
pub(crate) fn install_array_access_hook() {
    unsafe {
        runtime_hooks::install_array_access_hook(__elephc_eval_array_access_get as *const () as usize);
    }
}

/// Reads `$object[$key]` through an eval-declared class's `offsetGet()`.
///
/// `key_lo`/`key_hi` are the runtime's normalized key: an integer in `key_lo` with `key_hi == -1`,
/// or a string's pointer and length.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer, a string key must be readable
/// for `key_hi` bytes at `key_lo`, and `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_array_access_get(
    object: *mut c_void,
    key_lo: u64,
    key_hi: u64,
    out: *mut *mut RuntimeCell,
) -> u64 {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_array_access_get");
    std::panic::catch_unwind(|| unsafe { array_access_get_inner(object, key_lo, key_hi, out) })
        .unwrap_or(NOT_HANDLED)
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_array_access_get`.
unsafe fn array_access_get_inner(
    object: *mut c_void,
    key_lo: u64,
    key_hi: u64,
    out: *mut *mut RuntimeCell,
) -> u64 {
    if object.is_null() || out.is_null() {
        return NOT_HANDLED;
    }
    let identity = object as u64;
    let Some(context_ptr) = dynamic_object_owner_context(identity) else {
        return NOT_HANDLED;
    };
    let Some(context) = (unsafe { context_ptr.as_mut() }) else {
        return NOT_HANDLED;
    };
    let Some(class_name) = context
        .dynamic_object_class(identity)
        .map(|class| class.name().to_string())
    else {
        return NOT_HANDLED;
    };
    if context.class_method(&class_name, "offsetGet").is_none() {
        return NOT_HANDLED;
    }
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    let Ok(object_cell) = ElephcRuntimeOps::object_from_raw(object) else {
        return NOT_HANDLED;
    };
    let key = if key_hi == INT_KEY_SENTINEL {
        values.int(key_lo as i64)
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(key_lo as *const u8, key_hi as usize) };
        values.string_bytes_value(bytes)
    };
    let Ok(key) = key else {
        let _ = values.release(object_cell);
        return NOT_HANDLED;
    };
    let context = unsafe { &mut *context_ptr };
    let outcome = interpreter::execute_context_method_call_outcome(
        context,
        object_cell,
        "offsetGet",
        vec![key],
        &mut values,
    );
    let _ = values.release(key);
    let _ = values.release(object_cell);
    match outcome {
        Ok(EvalOutcome::Value(result)) => {
            unsafe { *out = result.as_ptr() };
            HANDLED
        }
        Ok(EvalOutcome::Throwable(error)) => {
            unsafe { *out = error.as_ptr() };
            THREW
        }
        Err(_) => NOT_HANDLED,
    }
}
