//! Purpose:
//! Hydrates an EVAL-DECLARED class for the generated `unserialize` decoder, which can only
//! build classes that have a compiled layout.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in
//!   `_elephc_eval_unserialize_object_fn`.
//!
//! Key details:
//! - `__rt_new_by_name` resolves against the AOT class table. A class the AUTOLOADER declared at
//!   run time has no entry there, so the decoder fell through to its `__PHP_Incomplete_Class`
//!   stand-in (class id -2) and the first typed parameter that object reached refused to bind.
//!   That is not an edge case on a Symfony request: the compiled DI container lives in
//!   `var/cache` behind a computed `require`, so it is interpreted, and every class it names is
//!   eval-declared.
//! - The decoder has already parsed the whole property list into its opaque hash and has
//!   already applied the `allowed_classes` policy, so the only question left here is whether
//!   the INTERPRETER declared the class. Anything else is answered with null and keeps the
//!   decoder's existing behaviour.
//! - OWNERSHIP: the hash stays the decoder's. Its values are read with `array_get`, which hands
//!   back fresh cells, and those are what move into the object — so the stand-in object the
//!   decoder frees afterwards still owns exactly the references it created.

use crate::ffi::util::abi_name_to_string;
use crate::interpreter::eval_unserialize_declared_object_from_hash;
use crate::interpreter::RuntimeValueOps;
use crate::runtime_hooks::{self, ElephcRuntimeOps};
use crate::value::RuntimeCell;

/// Installs the eval unserialize-object callback into the generated runtime.
pub(crate) fn install_unserialize_object_hook() {
    unsafe {
        runtime_hooks::install_unserialize_object_hook(
            __elephc_eval_unserialize_object as *const () as usize,
        );
    }
}

/// Builds one eval-declared object from a parsed property hash.
///
/// Returns a boxed Mixed object cell the caller then owns, or null when the class is not one the
/// interpreter declared — in which case the decoder keeps its incomplete-class fallback.
///
/// # Safety
/// `name_ptr` must be readable for `name_len` bytes, and `payload_hash` must be null or a live
/// elephc runtime hash whose values are boxed Mixed cells.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_unserialize_object(
    name_ptr: *const u8,
    name_len: u64,
    payload_hash: u64,
) -> *mut RuntimeCell {
    crate::ffi::util::trace_eval_ffi_entry("__elephc_eval_unserialize_object");
    std::panic::catch_unwind(|| unsafe {
        unserialize_object_inner(name_ptr, name_len, payload_hash)
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Executes the callback body after the exported ABI shim has installed a panic boundary.
///
/// # Safety
/// Mirrors `__elephc_eval_unserialize_object`.
unsafe fn unserialize_object_inner(
    name_ptr: *const u8,
    name_len: u64,
    payload_hash: u64,
) -> *mut RuntimeCell {
    let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
        return std::ptr::null_mut();
    };
    if name.is_empty() {
        return std::ptr::null_mut();
    }
    let context = crate::ffi::context::shared_null_handle_context();
    let context_ptr = context as *mut crate::abi::ElephcEvalContext;
    context.sync_global_eval_classes();
    let mut values = ElephcRuntimeOps::with_context(context_ptr);
    // The decoder hands the hash as a raw heap word; boxing it gives the ordinary array API to
    // walk it with, and the box is released below without touching the hash itself.
    let payload = if payload_hash == 0 {
        None
    } else {
        values.raw_heap_word_value(payload_hash).ok()
    };
    // Read BEFORE hydration and release: these two numbers separate "the interpreter built an
    // empty object" from "the decoder handed over a hash this side could not read".
    let traced = crate::eval_trace::enabled().then(|| {
        payload.map_or((u64::MAX, usize::MAX), |payload| {
            (
                values.type_tag(payload).unwrap_or(u64::MAX),
                values.array_len(payload).unwrap_or(usize::MAX),
            )
        })
    });
    let context = unsafe { &mut *context_ptr };
    let hydrated = eval_unserialize_declared_object_from_hash(
        &name,
        payload.unwrap_or(crate::value::RuntimeCellHandle::from_raw(std::ptr::null_mut())),
        context,
        &mut values,
    );
    if let Some(payload) = payload {
        let _ = values.release(payload);
    }
    if let Some((payload_tag, payload_len)) = traced {
        eprintln!(
            "[elephc-eval-trace] phase=unserialize_declared_object name={name:?} hydrated={} payload_tag={payload_tag} payload_len={payload_len}",
            matches!(hydrated, Ok(Some(_))),
        );
    }
    match hydrated {
        Ok(Some(object)) => object.as_ptr(),
        _ => std::ptr::null_mut(),
    }
}
