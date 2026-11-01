//! Purpose:
//! Registers a clone compiled code made of an eval-declared object with the source's context.
//!
//! Called from:
//! - Generated code, through the pointer installed in `_elephc_eval_object_clone_adopt_fn`, right
//!   after a compiled `clone` of an object whose class it cannot see. (`_elephc_eval_object_clone_fn`
//!   is the separate clone OPERATION hook, offered every object before the clone.)
//!
//! Key details:
//! - The compiled clone copies the native layout, which for an eval-declared class is its
//!   compiled ancestor's. Without this the clone was an instance of that ancestor, with none of
//!   the properties the eval class keeps in its context.
//! - A source no eval context owns is left alone: the compiled clone is already complete.

use crate::ffi::dynamic_destructors::dynamic_object_owner_context;
use crate::interpreter::{self, RuntimeValueOps};
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use std::ffi::c_void;

/// Installs the compiled-clone adoption callback into the generated runtime.
pub(crate) fn install_object_clone_hook() {
    unsafe {
        runtime_hooks::install_object_clone_hook(
            __elephc_eval_adopt_object_clone as *const () as usize,
        );
    }
}

/// Makes `clone` an object of `source`'s eval-declared class.
///
/// # Safety
/// `source` and `clone` must be null or live elephc runtime object pointers.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_adopt_object_clone(source: *mut c_void, clone: *mut c_void) {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_adopt_object_clone");
    let _ = std::panic::catch_unwind(|| unsafe { adopt_object_clone_inner(source, clone) });
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_adopt_object_clone`.
unsafe fn adopt_object_clone_inner(source: *mut c_void, clone: *mut c_void) {
    if source.is_null() || clone.is_null() || source == clone {
        return;
    }
    let source_identity = source as u64;
    let Some(context_ptr) = dynamic_object_owner_context(source_identity) else {
        return;
    };
    let Some(context) = (unsafe { context_ptr.as_mut() }) else {
        return;
    };
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    let Ok(clone_cell) = ElephcRuntimeOps::object_from_raw(clone) else {
        return;
    };
    let adopted =
        interpreter::adopt_compiled_object_clone(source_identity, clone_cell, context, &mut values);
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=adopt_compiled_clone source={source_identity} clone={} adopted={adopted:?}",
            clone as u64,
        );
    }
    // `object_from_raw` retained the borrowed clone for the call; give that back.
    let _ = values.release(clone_cell);
}
