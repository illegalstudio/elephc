//! Purpose:
//! Exposes module-generation catalog reuse without sharing activation-local eval state.
//!
//! Called from:
//! - Generated eval context initialization and native registration ABI wrappers.
//!
//! Key details:
//! - The writable module token is aligned, private to its loaded image and initially zero.
//! - A new generation prevents address reuse after module reload from selecting stale callbacks.
//! - Cached metadata is thread-local and never contains runtime cells or PHP scope state.

use crate::abi::{ElephcEvalContext, ABI_VERSION};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CATALOG_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Assigns one process-unique generation to a live module's zero-initialized token.
///
/// # Safety
/// `token` must be null or point to writable, aligned module-lifetime u64 storage.
unsafe fn module_generation(token: *mut u64) -> Option<u64> {
    if token.is_null() || (token as usize) % std::mem::align_of::<AtomicU64>() != 0 {
        return None;
    }
    let token = unsafe { &*token.cast::<AtomicU64>() };
    let current = token.load(Ordering::Acquire);
    if current != 0 { return Some(current); }
    let next = NEXT_CATALOG_GENERATION.fetch_update(Ordering::Relaxed, Ordering::Relaxed,
        |value| value.checked_add(1)).ok()?;
    Some(match token.compare_exchange(0, next, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => next,
        Err(existing) => existing,
    })
}

/// Allocates a fresh eval context and attaches this thread's cached module metadata.
///
/// # Safety
/// `token` must follow `module_generation`'s storage contract. Registered descriptors,
/// invokers and default factories must remain valid while the module is loaded and
/// while any context using that module remains live.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_new_with_native_catalog(
    token: *mut u64,
) -> *mut ElephcEvalContext {
    std::panic::catch_unwind(|| {
        let mut context = super::context::allocate_eval_context();
        if let Some(generation) = unsafe { module_generation(token) } {
            context.use_native_catalog(generation);
        }
        Box::into_raw(context)
    }).unwrap_or(std::ptr::null_mut())
}

/// Reports whether generated code may skip the complete native registration sequence.
///
/// # Safety
/// `ctx` must be null or a live eval context handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_native_catalog_ready(
    ctx: *mut ElephcEvalContext,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        ctx.as_ref().filter(|context| context.abi_version() == ABI_VERSION)
            .map_or(0, |context| i32::from(context.native_catalog_is_ready()))
    }).unwrap_or(0)
}

/// Publishes a fully registered native catalog before executing PHP in this context.
///
/// # Safety
/// `ctx` must be null or a live eval context handle on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_context_publish_native_catalog(
    ctx: *mut ElephcEvalContext,
) -> i32 {
    std::panic::catch_unwind(|| unsafe {
        ctx.as_mut().filter(|context| context.abi_version() == ABI_VERSION)
            .map_or(0, |context| i32::from(context.publish_native_catalog()))
    }).unwrap_or(0)
}

/// Marks a failed native registration without changing its existing Boolean ABI result.
///
/// # Safety
/// `ctx` must be null or a live context used by the preceding registration call.
pub(crate) unsafe fn record_registration_result(ctx: *mut ElephcEvalContext, status: i32) -> i32 {
    if status == 0 {
        if let Some(context) = unsafe { ctx.as_mut() } {
            context.fail_native_catalog_registration();
        }
    }
    status
}

/// Records a validated metadata write separately from its legacy insertion/replacement status.
///
/// # Safety
/// `ctx` must be null or the live context used by the preceding registration call.
pub(crate) unsafe fn record_registration_outcome(ctx: *mut ElephcEvalContext, outcome: Option<i32>) -> i32 {
    match outcome {
        Some(status) => status,
        None => unsafe { record_registration_result(ctx, 0) },
    }
}

#[cfg(test)]
mod tests;
