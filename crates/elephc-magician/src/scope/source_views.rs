//! Purpose:
//! Tracks binding-local producer snapshots and predicate-derived views independently of contracts.
//!
//! Called from:
//! - Assignment, parameter/capture binding, reference promotion and Mixed guard consumers.
//!
//! Key details:
//! - Binding generations and weak canonical write observations retire stale shared facts.
//! - Producer views preserve declarations; observed guard views can narrow those declarations.

use super::{ElephcEvalScope, LocalSourceView, LocalArgumentSource};

impl ElephcEvalScope {
    /// Records a current source view without adding it to the enforced local contracts.
    pub(crate) fn record_local_source_view(&mut self, name: String, ty: crate::eval_ir::EvalParameterType, epoch: u64) {
        if let Some(entry) = self.entry(&name).filter(|entry| entry.flags().is_visible()) {
            self.local_source_views.insert(name, LocalSourceView { generation: entry.generation(), write_epoch: epoch, ty, predicate_guard: false });
        }
    }

    /// Reads a generation-current source, requiring no intervening write to the same shared source.
    pub(crate) fn local_source_view(&self, name: &str, context: &crate::context::ElephcEvalContext) -> Option<&crate::eval_ir::EvalParameterType> {
        let entry = self.entry(name).filter(|entry| entry.flags().is_visible())?;
        let source = self.local_source_views.get(name)?;
        if source.generation != entry.generation() { return None; }
        let shared = entry.flags().by_ref || self.reference_target(name).is_some()
            || self.global_alias_target(name).is_some();
        if shared && !context.source_view_is_current(entry.cell(), source.write_epoch) { return None; }
        Some(&source.ty)
    }

    /// Stores nested tuple evidence at the successful binding publication generation.
    pub(crate) fn record_local_argument_source(&mut self, name: String,
        source: crate::source_snapshot::SourceSnapshot,
    ) {
        if let Some(entry) = self.entry(&name).filter(|entry| entry.flags().is_visible()) {
            self.local_argument_sources.insert(name, LocalArgumentSource { generation: entry.generation(), source });
        }
    }

    /// Reads captured entry proofs only while their containing binding remains unchanged.
    pub(crate) fn local_argument_source(&self, name: &str,
        context: &crate::context::ElephcEvalContext,
    ) -> Option<&crate::source_snapshot::SourceSnapshot> {
        let entry = self.entry(name).filter(|entry| entry.flags().is_visible())?;
        let source = self.local_argument_sources.get(name)?;
        if source.generation != entry.generation() { return None; }
        let shared = entry.flags().by_ref || self.reference_target(name).is_some()
            || self.global_alias_target(name).is_some();
        if shared && !context.source_view_is_current(entry.cell(), source.source.write_epoch) { return None; }
        Some(&source.source)
    }

    /// Records an actual predicate result independently from a declared producer's broad contract.
    pub(crate) fn record_local_guard_source_view(&mut self, name: String,
        ty: crate::eval_ir::EvalParameterType, epoch: u64,
    ) {
        self.record_local_source_view(name.clone(), ty, epoch);
        if let Some(view) = self.local_source_views.get_mut(&name) { view.predicate_guard = true; }
    }

    /// Gives an unmodified predicate fact precedence over an explicitly broad declaration.
    pub(crate) fn local_guard_source_view(&self, name: &str,
        context: &crate::context::ElephcEvalContext,
    ) -> Option<&crate::eval_ir::EvalParameterType> {
        self.local_source_views.get(name).filter(|view| view.predicate_guard)?;
        self.local_source_view(name, context)
    }

}
