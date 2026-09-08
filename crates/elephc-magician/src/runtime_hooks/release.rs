//! Purpose:
//! Releases native value owners without allowing destructor jumps through Rust cleanup frames.
//!
//! Called from:
//! - RuntimeValueOps releases, native argument cleanup, and native object-edge retirement.
//!
//! Key details:
//! - All owners are released, even if several destructors throw.
//! - The native boundary consumes and updates one owned Throwable accumulator.

use super::{ElephcRuntimeOps, EvalStatus, RuntimeCellHandle};

/// Consumes every supplied owner and returns the accumulated owned destructor exception, if any.
pub(crate) fn release_native_cells(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
) -> Option<RuntimeCellHandle> {
    let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    for cell in cells {
        unsafe { super::externs::__elephc_eval_value_release_v2(cell.as_ptr(), &mut thrown); }
    }
    (!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown))
}

impl ElephcRuntimeOps {
    /// Releases a group completely, chaining new failures with the context's already pending throw.
    pub(in crate::runtime_hooks) fn release_cells(
        &mut self,
        cells: impl IntoIterator<Item = RuntimeCellHandle>,
    ) -> Result<(), EvalStatus> {
        let pending = unsafe { (self.context as *mut super::ElephcEvalContext).as_mut() }
            .and_then(super::ElephcEvalContext::take_pending_throw);
        if let Some(thrown) = release_native_cells(cells, pending) {
            self.schedule_pending_throw(thrown)?;
            Err(EvalStatus::UncaughtThrowable)
        } else {
            Ok(())
        }
    }
}
