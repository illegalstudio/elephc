//! Purpose:
//! Selects owned incoming parameters whose names can be recreated by a conditional unset.
//!
//! Called from:
//! - The per-frame storage scan before checking the first user statement.
//!
//! Key details:
//! - Incoming ABI slots stay intact; source metadata requests a separate owning local.
//! - Foreign aliases and raw pointer layouts retain their existing storage paths.

use super::Facts;
use crate::types::{checker::Checker, PhpType};

impl Checker {
    /// Records entry contracts and physical store sites for eligible by-value parameters.
    pub(super) fn mark_recreatable_parameter_origins(&mut self, facts: &Facts, incoming: &crate::types::checker::PreBoundLocals) {
        if !self.strict_locals { return; }
        for (name, incoming) in incoming {
            let Some(source) = facts.names.get(name).filter(|source|
                source.conditional_unset && !source.disqualified) else { continue; };
            if !self.local_parameter_names.contains(name) || self.active_ref_params.contains(name)
                || self.name_is_seeded_program_storage(name)
                || matches!(incoming.storage.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) {
                continue;
            }
            self.local_store_contracts.record_entry_origin(self.current_loop_storage_scope.clone(),
                name.clone(), incoming.contract.clone());
            self.inferred_local_contracts.insert(name.clone(), incoming.contract.clone());
            self.mixed_storage_locals.insert(name.clone());
            for assignment in &source.assigns {
                self.mixed_storage_store_sites.entry(assignment.span).or_default().insert(name.clone());
                self.retired_mixed_storage_store_sites.remove(&(assignment.span, name.clone()));
            }
        }
    }
}
