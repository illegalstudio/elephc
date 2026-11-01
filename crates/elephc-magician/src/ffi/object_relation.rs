//! Purpose:
//! Answers class relations for the generated `__elephc_eval_value_is_a` helper when its AOT
//! metadata cannot.
//!
//! Called from:
//! - The generated runtime, through the pointer installed in `_elephc_eval_object_relation_fn`.
//!
//! Key details:
//! - `__elephc_eval_value_is_a` decides `instanceof` from the object header's class id alone. An
//!   object the INTERPRETER built carries `stdClass` there whatever class declared it, so every
//!   eval object failed an object type hint on an AOT method's parameter. That is not an edge
//!   case on a Symfony request: the compiled DI container is interpreted, so the loader it hands
//!   to the compiled kernel's `loadRoutes(LoaderInterface $loader)` is an eval object.
//! - RE-ENTRANCY is the whole reason this wrapper exists rather than installing
//!   `__elephc_eval_object_is_a` directly. The interpreter's answer for an eval class with a
//!   NATIVE parent is `values.object_is_a(...)`, which calls straight back into
//!   `__elephc_eval_value_is_a` -- and that now calls here on a false answer. The flag below
//!   bounds the cycle at one level: the inner call declines, the metadata matcher's own answer
//!   stands, and the outer call returns it.

use crate::value::RuntimeCell;
use std::cell::Cell;

thread_local! {
    /// Whether an interpreter relation answer is already being computed on this thread.
    static ANSWERING_RELATION: Cell<bool> = const { Cell::new(false) };
}

/// Installs the eval class-relation callback into the generated runtime.
pub(crate) fn install_object_relation_hook() {
    unsafe {
        crate::runtime_hooks::install_object_relation_hook(
            __elephc_eval_object_relation as *const () as usize,
        );
    }
}

/// Answers one class relation the generated AOT metadata could not.
///
/// Returns 1 when the interpreter knows the relation holds, and 0 for everything else --
/// including an object it does not own, which leaves the caller's metadata answer standing.
///
/// # Safety
/// `object` must be null or a live boxed runtime cell, and `target_ptr` must be readable for
/// `target_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_object_relation(
    object: *mut RuntimeCell,
    target_ptr: *const u8,
    target_len: u64,
    exclude_self: u64,
) -> u64 {
    if object.is_null() || target_ptr.is_null() || target_len == 0 {
        return 0;
    }
    if ANSWERING_RELATION.with(|answering| answering.replace(true)) {
        return 0;
    }
    let answer = std::panic::catch_unwind(|| unsafe {
        crate::ffi::object_introspection::__elephc_eval_object_is_a(
            std::ptr::null_mut(),
            object,
            target_ptr,
            target_len,
            exclude_self,
        )
    })
    .unwrap_or(0);
    ANSWERING_RELATION.with(|answering| answering.set(false));
    // 1 is the only "yes": `__elephc_eval_object_is_a` answers 2 for an object the interpreter
    // owns that is definitely NOT an instance, which is a "no" here like any other.
    u64::from(answer == 1)
}
