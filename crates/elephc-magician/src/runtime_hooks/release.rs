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

/// Consumes every supplied owner and returns the accumulated owned destructor exception, if any.
pub(crate) fn release_native_cells(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
) -> Option<RuntimeCellHandle> {
    release_native_cells_with_status(cells, pending).0
}

/// Separates new cleanup failures from the exception already escaping through the caller.
fn release_native_cells_with_status(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
) -> (Option<RuntimeCellHandle>, bool) {
    let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    let mut caught = false;
    for cell in cells {
        let status = unsafe { super::externs::__elephc_eval_value_release_v3(cell.as_ptr(), &mut thrown) };
        caught |= status != 0;
    }
    ((!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown)), caught)
}

impl ElephcRuntimeOps {
    /// Releases a group completely, chaining new failures with the context's already pending throw.
    pub(in crate::runtime_hooks) fn release_cells(
        &mut self,
        cells: impl IntoIterator<Item = RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let pending = unsafe { (self.context as *mut super::ElephcEvalContext).as_mut() }
            .and_then(super::ElephcEvalContext::take_pending_throw);
        let (thrown, caught) = release_native_cells_with_status(cells, pending);
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
