//! Purpose:
//! Selects incoming parameter bindings whose names can be recreated by a plain unset.
//!
//! Called from:
//! - The per-frame storage scan before checking the first user statement.
//!
//! Key details:
//! - By-value inputs use owning shadows; reference inputs keep their original binding handles.
//! - Payload facts remain independent of source contracts and raw pointer layouts stay excluded.

use super::Facts;
use crate::types::{checker::Checker, PhpType};

impl Checker {
    /// Keeps source origins on owned eval-visible names without changing foreign reference ABIs.
    pub(super) fn mark_owned_eval_binding_origins(&mut self, facts: &Facts,
        incoming: &crate::types::checker::PreBoundLocals) {
        if !self.strict_locals || !facts.contains_eval { return; }
        for (name, source) in &facts.names {
            if !source.has_unset || source.disqualified || incoming.contains_key(name)
                || self.active_ref_params.contains(name) || self.local_binding_depth.contains_key(name)
                || self.typed_local_names.contains_key(name) || self.active_globals.contains(name)
                || self.static_local_names.contains(name) || self.name_is_seeded_program_storage(name)
                || self.top_level_binding_is_program_global(name) || name == "this"
                || source.assigns.iter().any(|site| !site.span.identifies_a_node()) {
                continue;
            }
            self.local_store_contracts.track_binding_origin(&self.current_loop_storage_scope, name);
            self.mixed_storage_locals.insert(name.clone());
            for assignment in &source.assigns {
                self.mixed_storage_store_sites.entry(assignment.span).or_default().insert(name.clone());
                self.retired_mixed_storage_store_sites.remove(&(assignment.span, name.clone()));
            }
        }
    }

    /// Records entry contracts and store sites without conflating value and reference inputs.
    pub(super) fn mark_recreatable_parameter_origins(&mut self, facts: &Facts, incoming: &crate::types::checker::PreBoundLocals) {
        if !self.strict_locals { return; }
        for (name, incoming) in incoming {
            let Some(source) = facts.names.get(name).filter(|source|
                (source.conditional_unset || (facts.contains_eval && source.has_unset))
                    && !source.disqualified) else { continue; };
            if !self.local_parameter_names.contains(name)
                || self.name_is_seeded_program_storage(name)
                || matches!(incoming.storage.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) {
                continue;
            }
            if self.active_ref_params.contains(name) {
                self.local_store_contracts.record_reference_entry_origin(self.current_loop_storage_scope.clone(),
                    name.clone(), incoming.contract.clone());
            } else {
                self.local_store_contracts.record_entry_origin(self.current_loop_storage_scope.clone(),
                    name.clone(), incoming.contract.clone());
            }
            self.inferred_local_contracts.insert(name.clone(), incoming.contract.clone());
            self.mixed_storage_locals.insert(name.clone());
            for assignment in &source.assigns {
                self.mixed_storage_store_sites.entry(assignment.span).or_default().insert(name.clone());
                self.retired_mixed_storage_store_sites.remove(&(assignment.span, name.clone()));
            }
        }
    }
}
