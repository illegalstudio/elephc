//! Purpose:
//! Keeps incoming frame storage separate from PHP-visible binding contracts.
//!
//! Called from:
//! - Function, method and closure body setup before the shared storage scan.
//!
//! Key details:
//! - An unannotated parameter stays open even when inference narrows its checker view.
//! - Variadic collectors always have an array contract; captures retain source contracts separately from storage evidence.

use crate::types::PhpType;
use std::collections::HashMap;

/// One value already present in the body's own frame at activation entry.
pub(crate) struct PreBoundLocal {
    pub storage: PhpType,
    pub contract: PhpType,
    pub runtime_contract: bool,
}

/// Incoming locals indexed by their source names, without importing the outer environment.
pub(crate) type PreBoundLocals = HashMap<String, PreBoundLocal>;

impl PreBoundLocal {
    /// Separates the inferred checker entry view from a declared or open parameter contract.
    pub(crate) fn parameter(storage: &PhpType, declared: bool) -> Self {
        Self { storage: storage.clone(), contract: if declared { storage.clone() } else { PhpType::Mixed }, runtime_contract: false }
    }

    /// Keeps a capture snapshot layout separate from its enclosing producer contract.
    pub(crate) fn capture(storage: &PhpType, contract: PhpType, runtime_contract: bool) -> Self {
        Self { storage: storage.clone(), contract, runtime_contract }
    }
}

impl super::Checker {
    /// Seeds incoming source contracts independently of inferred or narrowed checker views.
    pub(super) fn seed_entry_binding_contracts(&mut self, incoming: &PreBoundLocals) {
        if !self.strict_locals { return; }
        for (name, entry) in incoming {
            if entry.contract == PhpType::Mixed && matches!(entry.storage.codegen_repr(),
                PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)) { continue; }
            self.inferred_local_contracts.insert(name.clone(), entry.contract.clone());
        }
    }

    /// Keeps inferred views from closing an open binding, including externally coerced aliases.
    pub(super) fn open_binding_store_view(&self, name: &str, incoming: &PhpType) -> Option<PhpType> {
        (self.strict_locals
            && self.inferred_local_contracts.get(name) == Some(&PhpType::Mixed)
            && !self.typed_local_names.contains_key(name) && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name))
            .then(|| if self.local_binding_is_unaliased(name) { incoming.clone() } else { PhpType::Mixed })
    }
}
