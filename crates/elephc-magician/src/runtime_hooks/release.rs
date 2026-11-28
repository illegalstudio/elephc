//! Purpose:
//! Releases native value owners without allowing destructor jumps through Rust cleanup frames.
//!
//! Called from:
//! - RuntimeValueOps releases, native argument cleanup, and native object-edge retirement.
//!
//! Key details:
//! - All owners are released, even if several destructors throw.
//! - The native boundary consumes and updates one owned Throwable accumulator.
//! - A pre-existing throw is preserved without turning successful cleanup or writeback into a failure.

use super::{ElephcRuntimeOps, EvalStatus, RuntimeCellHandle};
use elephc_memoir::runtime_owners::{
    CleanupErrorPolicy, ReleaseOrder, RuntimeOwnerBatch,
};

/// Consumes every supplied owner and returns the accumulated owned destructor exception, if any.
pub(crate) fn release_native_cells(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
) -> Option<RuntimeCellHandle> {
    release_native_owner_batch(RuntimeOwnerBatch::new(cells), pending)
}

/// Releases one detached Memoir batch while its outstanding slots remain collector roots.
pub(crate) fn release_native_owner_batch(
    cells: RuntimeOwnerBatch,
    pending: Option<RuntimeCellHandle>,
) -> Option<RuntimeCellHandle> {
    release_native_owner_batch_with_status(cells, pending).0
}

/// Releases a rooted batch while reporting whether cleanup caught a new native exception.
fn release_native_owner_batch_with_status(
    cells: RuntimeOwnerBatch,
    pending: Option<RuntimeCellHandle>,
) -> (Option<RuntimeCellHandle>, bool) {
    let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    let mut caught = false;
    super::owner_frames::with_runtime_owners(&cells, || {
        let released: Result<(), std::convert::Infallible> = cells.release(
            ReleaseOrder::Forward,
            CleanupErrorPolicy::Ignore,
            |cell| {
                caught |= unsafe {
                    super::externs::__elephc_eval_value_release_v3(cell.as_ptr(), &mut thrown)
                } != 0;
                Ok(())
            },
        );
        let _ = released;
    });
    (
        (!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown)),
        caught,
    )
}

impl ElephcRuntimeOps {
    /// Releases a detached group while every not-yet-retired owner remains collector-visible.
    pub(in crate::runtime_hooks) fn release_owner_batch(
        &mut self,
        cells: RuntimeOwnerBatch,
    ) -> Result<(), EvalStatus> {
        let pending = unsafe { (self.context as *mut super::ElephcEvalContext).as_mut() }
            .and_then(super::ElephcEvalContext::take_pending_throw);
        let (thrown, caught) = release_native_owner_batch_with_status(cells, pending);
        self.finish_native_cleanup(thrown, caught)
    }

    /// Releases a group completely, chaining new failures with the context's already pending throw.
    pub(in crate::runtime_hooks) fn release_cells(
        &mut self,
        cells: impl IntoIterator<Item = RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        self.release_owner_batch(RuntimeOwnerBatch::new(cells))
    }

    /// Unlinks a staging record before consuming its owner inside the native cleanup boundary.
    pub(in crate::runtime_hooks) fn release_reference_pin(
        &mut self, pin: *mut std::ffi::c_void,
    ) -> Result<(), EvalStatus> {
        let pending = unsafe { (self.context as *mut super::ElephcEvalContext).as_mut() }
            .and_then(super::ElephcEvalContext::take_pending_throw);
        let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
        let caught = unsafe {
            super::externs::__elephc_eval_value_native_reference_pin_release_v1(pin, &mut thrown)
        } != 0;
        self.finish_native_cleanup(
            (!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown)), caught,
        )
    }

    /// Republishes an owned pending chain while distinguishing an earlier throw from a new failure.
    fn finish_native_cleanup(
        &mut self, thrown: Option<RuntimeCellHandle>, caught: bool,
    ) -> Result<(), EvalStatus> {
        let has_throwable = thrown.is_some();
        if let Some(thrown) = thrown {
            self.schedule_pending_throw(thrown)?;
        }
        match (caught, has_throwable) {
            (true, true) => Err(EvalStatus::UncaughtThrowable),
            (true, false) => Err(EvalStatus::RuntimeFatal),
            (false, _) => Ok(()),
        }
    }
}
