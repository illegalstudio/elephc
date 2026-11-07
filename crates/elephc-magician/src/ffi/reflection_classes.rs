//! Purpose:
//! Exports the eval half of the lazy declaring-class getters: builds a fresh, complete
//! `ReflectionClass` for the class name an eval-built member reflector stores.
//!
//! Called from:
//! - Generated EIR backend assembly for the internal `__elephc_eval_reflection_class_build`
//!   builtin, which the compiled `getDeclaringClass()` / `ReflectionParameter::getClass()`
//!   getters call when their slot holds a class name.
//!
//! Key details:
//! - The getters own no eval local, so the context is the active interpreter frame's, the
//!   request owner declaring the class, or (for a compiled class) any live request owner.
//! - A reflector can outlive every context that saw its class (it escaped the function whose
//!   eval declared it). Eval declarations are process-global, so the build then runs in a
//!   transient context attached to the module's native catalog, freed before returning.
//! - Every call registers a new reflector identity, so no caller ever receives a copy the eval
//!   reflection registry does not know.
//! - Returns -1 when no context can be created; the generated caller then yields `false`.

use super::util::{abi_name_to_string, clear_result, write_outcome};
use crate::abi::{ElephcEvalContext, ElephcEvalResult, ABI_VERSION};
use crate::context::{
    any_live_native_context, live_native_context_for_class, native_frame_active_eval_context,
};
use crate::errors::EvalStatus;
use crate::interpreter;
use crate::runtime_hooks::ElephcRuntimeOps;

/// Builds a fresh, complete `ReflectionClass` for the class named by `name_ptr`/`name_len`.
///
/// # Safety
/// `catalog_token` must follow the module-token contract of
/// `__elephc_eval_context_new_with_native_catalog`. `name_ptr` must be readable for
/// `name_len` bytes when `name_len > 0`, and `out` may be null.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_reflection_class_build_v1(
    catalog_token: *mut u64,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        reflection_class_build_inner(catalog_token, name_ptr, name_len, out)
    })
    .unwrap_or_else(|_| EvalStatus::RuntimeFatal.code())
}

/// Resolves the owning context, or a transient one, and materializes the reflector into `out`.
///
/// # Safety
/// Pointer validity is inherited from `__elephc_eval_reflection_class_build_v1`.
unsafe fn reflection_class_build_inner(
    catalog_token: *mut u64,
    name_ptr: *const u8,
    name_len: u64,
    out: *mut ElephcEvalResult,
) -> i32 {
    unsafe {
        clear_result(out);
        let Ok(name) = abi_name_to_string(name_ptr, name_len) else {
            return EvalStatus::RuntimeFatal.code();
        };
        let live = native_frame_active_eval_context()
            .or_else(|| live_native_context_for_class(&name))
            .or_else(|| any_live_native_context());
        if let Some(context) = live {
            return build_in_context(context, &name, out);
        }
        let transient = super::native_catalog::__elephc_eval_context_new_with_native_catalog(
            catalog_token,
        );
        if transient.is_null() {
            return -1;
        }
        let status = build_in_context(transient, &name, out);
        // The transient context owns no PHP objects with destructors, so its teardown cannot
        // leave a pending throwable worth reporting; the built reflector is already in `out`.
        let _ = super::context::__elephc_eval_context_free(transient);
        status
    }
}

/// Builds the reflector in one live context and writes its outcome.
///
/// # Safety
/// `context` must be a live eval context handle on its owning thread.
unsafe fn build_in_context(
    context: *mut ElephcEvalContext,
    name: &str,
    out: *mut ElephcEvalResult,
) -> i32 {
    unsafe {
        let Some(context) = context.as_mut() else {
            return -1;
        };
        if context.abi_version() != ABI_VERSION {
            return EvalStatus::AbiMismatch.code();
        }
        context.sync_global_eval_classes();
        let mut values = ElephcRuntimeOps::with_context(context as *const ElephcEvalContext);
        match interpreter::execute_context_reflection_class_outcome(context, name, &mut values) {
            Ok(outcome) => write_outcome(outcome, out).code(),
            Err(status) => status.code(),
        }
    }
}
