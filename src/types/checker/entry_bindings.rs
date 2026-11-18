//! Purpose:
//! Keeps incoming frame storage separate from PHP-visible parameter binding contracts.
//!
//! Called from:
//! - Function, method and closure body setup before the shared storage scan.
//!
//! Key details:
//! - An unannotated parameter stays open even when inference narrows its checker view.
//! - Variadic collectors always have an array contract; capture storage evidence is retained.

use crate::types::PhpType;
use std::collections::HashMap;

/// One value already present in the body's own frame at activation entry.
pub(crate) struct PreBoundLocal {
    pub storage: PhpType,
    pub contract: PhpType,
}

/// Incoming locals indexed by their source names, without importing the outer environment.
pub(crate) type PreBoundLocals = HashMap<String, PreBoundLocal>;

impl PreBoundLocal {
    /// Separates the inferred checker entry view from a declared or open parameter contract.
    pub(crate) fn parameter(storage: &PhpType, declared: bool) -> Self {
        Self { storage: storage.clone(), contract: if declared { storage.clone() } else { PhpType::Mixed } }
    }

    /// Retains the capture evidence already used by whole-frame storage selection.
    pub(crate) fn capture(storage: &PhpType) -> Self {
        Self { storage: storage.clone(), contract: storage.clone() }
    }
}

impl super::Checker {
    /// Seeds source contracts for every explicit parameter independently of their inferred checker entry views.
    pub(super) fn seed_parameter_binding_contracts(&mut self, incoming: &PreBoundLocals) {
        if !self.strict_locals { return; }
        for name in &self.local_parameter_names {
            if let Some(entry) = incoming.get(name) {
                if entry.contract == PhpType::Mixed && matches!(entry.storage.codegen_repr(),
                    PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) { continue; }
                self.inferred_local_contracts.insert(name.clone(), entry.contract.clone());
            }
        }
    }

    /// Keeps inferred input views from closing an open parameter, including externally coerced aliases.
    pub(super) fn open_parameter_store_view(&self, name: &str, incoming: &PhpType) -> Option<PhpType> {
        (self.strict_locals && self.local_parameter_names.contains(name)
            && self.inferred_local_contracts.get(name) == Some(&PhpType::Mixed)
            && !self.typed_local_names.contains_key(name) && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name))
            .then(|| if self.local_binding_is_unaliased(name) { incoming.clone() } else { PhpType::Mixed })
    }
}
