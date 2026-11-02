//! Purpose:
//! Renders an EVAL-OWNED object for the generated `serialize()`, which can only read an object's
//! class from its header and its properties from the runtime hash.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in `_elephc_eval_serialize_object_fn`.
//!
//! Key details:
//! - Every object the interpreter builds carries the `stdClass` class id and an empty property
//!   hash, because its declared properties live in the eval context's overlay. The generated
//!   serializer therefore wrote `O:8:"stdClass":0:{}` for one -- which is what Symfony's
//!   `ConfigCache::write()` put in its routing cache metadata (that method is AOT-compiled, so
//!   its `serialize()` is the generated one), and what the next request read back as a bare
//!   stdClass before refusing it at a `ResourceInterface` parameter.
//! - Only an object with a REGISTERED OWNER context is answered. A native object has none, so
//!   the decision costs one registry lookup on a path that already walks an object header, and
//!   the generated serializer keeps its own rendering for everything it can already render.
//! - LIMITATION: the fragment is rendered by the interpreter's own serializer, which numbers the
//!   `r:<index>` back-references of anything NESTED inside the object independently of the
//!   generated serializer's table. An object repeated both inside and outside a delegated
//!   subtree would not be emitted as a back-reference. Everything else round-trips.

use crate::ffi::dynamic_destructors::dynamic_object_owner_context;
use crate::interpreter::eval_serialize_object_fragment;
use crate::interpreter::RuntimeValueOps;
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use crate::value::RuntimeCell;
use std::ffi::c_void;

/// Installs the eval serialize-object callback into the generated runtime.
pub(crate) fn install_serialize_object_hook() {
    unsafe {
        runtime_hooks::install_serialize_object_hook(
            __elephc_eval_serialize_object as *const () as usize,
        );
    }
}

/// Renders one eval-owned object's `O:len:"Class":n:{…}` fragment.
///
/// Returns a boxed Mixed string the caller then owns and must release, or null when the object is
/// not one the interpreter built -- in which case the generated serializer keeps its own
/// rendering.
///
/// # Safety
/// `object` must be null or a live elephc runtime object pointer.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_serialize_object(object: *mut c_void) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_serialize_object");
    std::panic::catch_unwind(|| unsafe { serialize_object_inner(object) })
        .unwrap_or(std::ptr::null_mut())
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_serialize_object`.
unsafe fn serialize_object_inner(object: *mut c_void) -> *mut RuntimeCell {
    if object.is_null() {
        return std::ptr::null_mut();
    }
    // The eval identity of an object IS its payload pointer, so the registry answers without
    // boxing anything first.
    let identity = object as u64;
    let Some(context) = dynamic_object_owner_context(identity) else {
        return std::ptr::null_mut();
    };
    let Some(context) = (unsafe { context.as_mut() }) else {
        return std::ptr::null_mut();
    };
    if context.dynamic_object_declaring_class(identity).is_none() {
        return std::ptr::null_mut();
    }
    let context_ptr = context as *mut crate::abi::ElephcEvalContext;
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    let Ok(object_cell) = ElephcRuntimeOps::object_from_raw(object) else {
        return std::ptr::null_mut();
    };
    let context = unsafe { &mut *context_ptr };
    let fragment = eval_serialize_object_fragment(object_cell, context, &mut values);
    // `object_from_raw` retained the borrowed object for the call; give that back whichever way
    // the rendering went.
    let _ = values.release(object_cell);
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=serialize_eval_object identity={identity} rendered={}",
            fragment.is_ok(),
        );
    }
    match fragment {
        Ok(fragment) => fragment.as_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}
