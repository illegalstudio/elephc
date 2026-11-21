//! Purpose:
//! Selects incoming parameter and capture bindings whose names can be recreated by a plain unset.
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
    pub(super) fn mark_entry_binding_origins(&mut self, facts: &Facts, incoming: &crate::types::checker::PreBoundLocals) {
        if !self.strict_locals { return; }
        for (name, incoming) in incoming {
            let source = facts.names.get(name).filter(|source|
                !source.disqualified && (source.conditional_unset
                    || (source.has_unset && (facts.contains_eval || !self.local_parameter_names.contains(name)))));
            if !incoming.runtime_contract && source.is_none() { continue; }
            if name == "this" || self.name_is_seeded_program_storage(name)
                || matches!(incoming.storage.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) {
                continue;
            }
            self.record_frame_entry_origin(name, incoming);
            self.inferred_local_contracts.insert(name.clone(), incoming.contract.clone());
            self.mixed_storage_locals.insert(name.clone());
            for assignment in facts.names.get(name).into_iter().flat_map(|source| &source.assigns) {
                self.mixed_storage_store_sites.entry(assignment.span).or_default().insert(name.clone());
                self.retired_mixed_storage_store_sites.remove(&(assignment.span, name.clone()));
            }
        }
    }

    /// Tracks activation detachment independently of persistent static or program storage.
    pub(super) fn mark_persistent_binding_origins(&mut self, facts: &Facts, incoming: &crate::types::checker::PreBoundLocals) {
        if !self.strict_locals || self.null_probe_scope_is_top_level { return; }
        for (name, source) in &facts.names {
            if !(source.static_binding || source.global_binding) || !source.has_unset
                || !(source.conditional_unset || facts.contains_eval)
                || source.assigns.iter().any(|site| !site.span.identifies_a_node()) { continue; }
            if source.global_binding && (crate::superglobals::is_superglobal(name)
                || self.extern_globals.contains_key(name) || name == "this") { continue; }
            if let Some(entry) = incoming.get(name) { self.record_frame_entry_origin(name, entry); }
            self.local_store_contracts.track_binding_origin(&self.current_loop_storage_scope, name);
            self.mixed_storage_locals.insert(name.clone());
            for assignment in &source.assigns {
                self.mixed_storage_store_sites.entry(assignment.span).or_default().insert(name.clone());
                self.retired_mixed_storage_store_sites.remove(&(assignment.span, name.clone()));
            }
        }
    }

    /// Preserves a parameter or capture's original contract before a later declaration replaces it.
    fn record_frame_entry_origin(&mut self, name: &str, incoming: &crate::types::checker::PreBoundLocal) {
        let scope = self.current_loop_storage_scope.clone();
        if incoming.runtime_contract {
            self.local_store_contracts.record_captured_entry_origin(scope, name.into(),
                incoming.contract.clone(), self.active_ref_params.contains(name));
        } else if self.active_ref_params.contains(name) {
            self.local_store_contracts.record_reference_entry_origin(scope, name.into(), incoming.contract.clone());
        } else {
            self.local_store_contracts.record_entry_origin(scope, name.into(), incoming.contract.clone());
        }
    }
}
