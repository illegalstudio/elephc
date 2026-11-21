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
    interpreter::{eval_validate_property_reference, prepare_property_reference_candidates,
        coerce_property_reference_candidates, RuntimeValueOps},
    runtime_hooks::ElephcRuntimeOps, value::{RuntimeCell, RuntimeCellHandle}};
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use super::property_reference_context::ReferenceContextLease;

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
                return export_pending_throw(status, context, &mut values, thrown);
            }
        }
        0
    }).unwrap_or(1)
}

/// Returns one independently prepared eval declaration candidate to the native coordinator.
///
/// # Safety
/// Reference and original are borrowed boxes pinned by the publisher. Output slots are writable
/// and empty. Registered contexts and the active execution context belong to this thread.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_reference_coerce_v1(
    reference: *mut RuntimeCell, original: *mut RuntimeCell,
    candidate: *mut *mut RuntimeCell, thrown: *mut *mut RuntimeCell,
) -> u64 {
    if reference.is_null() || original.is_null() || candidate.is_null() || thrown.is_null() { return 1; }
    std::panic::catch_unwind(|| unsafe {
        candidate.write(std::ptr::null_mut());
        let reference = RuntimeCellHandle::from_raw(reference);
        let original = RuntimeCellHandle::from_raw(original);
        let contexts = live_native_reference_contexts();
        let mut prepared = Vec::new();
        // Snapshot all declarations before any coercion can execute a callback or retire a source.
        for pointer in &contexts {
            let Some(context) = pointer.as_mut() else { continue; };
            let mut values = ElephcRuntimeOps::with_context(*pointer);
            match prepare_property_reference_candidates(reference, original, context, &mut values) {
                Ok(plans) => prepared.extend(plans),
                Err(_) => return 1,
            }
        }
        if prepared.is_empty() { return 0; }
        let mut probe = ElephcRuntimeOps::new();
        let object_owner = if probe.type_tag(original) == Ok(6) {
            probe.object_identity(original).ok().and_then(
                crate::ffi::dynamic_destructors::dynamic_object_owner_context)
        } else { None };
        let pointer = object_owner.or_else(crate::context::native_frame_active_eval_context)
            .or_else(|| contexts.first().copied()).unwrap();
        let Ok(lease) = ReferenceContextLease::acquire(pointer) else { return 1; };
        let Some(context) = pointer.as_mut() else { return 1; };
        let mut values = ElephcRuntimeOps::with_context(pointer);
        let selected = RuntimeOwnerSlot::new(RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed());
        with_runtime_owners(&selected, || {
            let succeeded = match coerce_property_reference_candidates(prepared, original, context, &mut values) {
                Ok(Some(value)) => { selected.initialize(value); true }
                Ok(None) => true,
                Err(_) => false,
            };
            // Context teardown may run PHP; the selected owner stays rooted until it completes.
            let error = lease.finish();
            if let Some(error) = error {
                let retired = selected.take();
                let retired = (!retired.is_null()).then_some(retired);
                let error = crate::runtime_hooks::release::release_native_cells(retired, Some(error));
                let Some(error) = error else { return 1; };
                return export_throwable(error, &mut ElephcRuntimeOps::new(), thrown);
            }
            if !succeeded { return 1; }
            candidate.write(selected.take().as_ptr());
            0
        })
    }).unwrap_or(1)
}

/// Transfers a boxed eval Throwable as the raw object expected by the native validation context.
unsafe fn export_pending_throw(status: EvalStatus, context: &mut crate::context::ElephcEvalContext,
    values: &mut impl RuntimeValueOps, thrown: *mut *mut RuntimeCell) -> u64 {
    if status != EvalStatus::UncaughtThrowable { return 1; }
    let Some(error) = context.take_pending_throw() else { return 1; };
    unsafe { export_throwable(error, values, thrown) }
}

/// Converts one owned boxed Throwable without using a possibly retired execution context.
unsafe fn export_throwable(error: RuntimeCellHandle, values: &mut impl RuntimeValueOps,
    thrown: *mut *mut RuntimeCell) -> u64 {
    let raw = match values.raw_value_word(error).and_then(|raw| values.retain_raw_heap_word(raw)) {
        Ok(raw) => raw,
        Err(_) => { let _ = values.release(error); return 1; }
    };
    if values.release(error).is_err() { let _ = values.release_raw_heap_word(raw); return 1; }
    unsafe { thrown.write(raw as *mut RuntimeCell); }
    2
}
