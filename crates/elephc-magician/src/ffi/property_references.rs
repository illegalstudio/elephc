//! Purpose:
//! Checks eval-owned property sources before a native canonical reference publication.
//!
//! Called from:
//! - Generated native reference validation when the eval bridge and Coercions are selected.
//!
//! Key details:
//! - Live request contexts contribute weak property associations even after eval returns.
//! - No reference or candidate owner is consumed; failure transfers one raw Throwable owner.
//! - PHP callbacks use ordinary protected runtime operations and no native throw crosses Rust.

use crate::{context::live_native_reference_contexts, errors::EvalStatus,
    interpreter::{eval_validate_property_reference, RuntimeValueOps},
    runtime_hooks::ElephcRuntimeOps, value::{RuntimeCell, RuntimeCellHandle}};

/// Returns success0, invalid1 or contained Throwable2 before native storage changes.
///
/// # Safety
/// Reference and candidate are live borrowed boxes pinned by the native publisher. The output
/// points to an empty writable raw-object slot and every registered context lives on this thread.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_reference_validate_v1(
    reference: *mut RuntimeCell, candidate: *mut RuntimeCell, thrown: *mut *mut RuntimeCell,
) -> u64 {
    if reference.is_null() || candidate.is_null() || thrown.is_null() { return 1; }
    std::panic::catch_unwind(|| unsafe {
        let reference = RuntimeCellHandle::from_raw(reference);
        let candidate = RuntimeCellHandle::from_raw(candidate);
        for pointer in live_native_reference_contexts() {
            let Some(context) = pointer.as_mut() else { continue; };
            let mut values = ElephcRuntimeOps::with_context(pointer);
            if let Err(status) = eval_validate_property_reference(reference, candidate, context, &mut values) {
                if status != EvalStatus::UncaughtThrowable { return 1; }
                let Some(error) = context.take_pending_throw() else { return 1; };
                let raw = match values.raw_value_word(error)
                    .and_then(|raw| values.retain_raw_heap_word(raw)) {
                    Ok(raw) => raw,
                    Err(_) => { let _ = values.release(error); return 1; }
                };
                let released = values.release(error);
                if released.is_err() { let _ = values.release_raw_heap_word(raw); return 1; }
                thrown.write(raw as *mut RuntimeCell);
                return 2;
            }
        }
        0
    }).unwrap_or(1)
}
