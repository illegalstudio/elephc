//! Purpose:
//! Retires transient source snapshots when their own shared reference changes.
//!
//! Called from:
//! - Source snapshots, parameter/capture binding and canonical reference publication.
//!
//! Key details:
//! - Write epochs contain no types or PHP owners; source proofs stay on scope bindings.
//! - Existing allocation tokens retire write observations before reference address reuse.

use super::ElephcEvalContext;
use crate::value::RuntimeCellHandle;
use crate::ffi::array_references::{register_array_reference_cell, ArrayReferenceCellLifetime};

/// A weak observation of the latest canonical write, without source type information.
pub(super) struct SourceWriteEpoch {
    epoch: u64,
    lifetime: ArrayReferenceCellLifetime,
}

impl ElephcEvalContext {
    /// Stamps a source snapshot after its expression effects have finished.
    pub(crate) fn source_view_epoch(&self) -> u64 { self.source_view_epoch }

    /// Tests only writes to this source, so unrelated reference stores preserve its alternatives.
    pub(crate) fn source_view_is_current(&self, cell: RuntimeCellHandle, epoch: u64) -> bool {
        self.source_view_writes.get(&(cell.as_ptr() as usize))
            .filter(|write| write.lifetime.is_live()).is_none_or(|write| write.epoch <= epoch)
    }

    /// Retires snapshots before the canonical reference candidate can execute observable effects.
    pub(crate) fn invalidate_shared_source_views(&mut self, cell: RuntimeCellHandle) {
        use elephc_semantics::StrictFeature;
        if ![StrictFeature::ArrayTypes, StrictFeature::Mixed].iter()
            .any(|feature| self.semantic_profile().contains(*feature)) { return; }
        self.source_view_epoch = self.source_view_epoch.checked_add(1)
            .expect("shared source epoch exhausted");
        for address in self.source_view_retirements.take() { self.source_view_writes.remove(&address); }
        let address = cell.as_ptr() as usize;
        let entry = self.source_view_writes.entry(address).or_insert_with(|| SourceWriteEpoch {
            epoch: 0,
            lifetime: register_array_reference_cell(address, &self.source_view_retirements),
        });
        entry.epoch = self.source_view_epoch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_semantics::{SemanticProfile, StrictFeature};

    /// Only the written live allocation retires a stamp; address reuse starts a fresh observation.
    #[test]
    fn source_view_epochs_follow_exact_reference_writes_and_retirement() {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE
            .with(StrictFeature::ArrayTypes, true)).unwrap();
        let first_allocation = Box::new(0_u8); let second_allocation = Box::new(0_u8);
        let first = RuntimeCellHandle::from_raw((&*first_allocation as *const u8).cast_mut().cast());
        let second = RuntimeCellHandle::from_raw((&*second_allocation as *const u8).cast_mut().cast());
        let before = context.source_view_epoch();
        context.invalidate_shared_source_views(first);
        assert!(!context.source_view_is_current(first, before));
        assert!(context.source_view_is_current(second, before));
        let after = context.source_view_epoch();
        assert!(context.source_view_is_current(first, after));
        crate::ffi::array_references::retire_array_reference_cell(first.as_ptr() as usize);
        assert!(context.source_view_is_current(first, before));
        context.invalidate_shared_source_views(first);
        assert!(!context.source_view_is_current(first, after));
        crate::ffi::array_references::retire_array_reference_cell(first.as_ptr() as usize);
    }
}
