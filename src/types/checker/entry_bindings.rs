//! Purpose:
//! Keeps incoming frame storage separate from PHP-visible parameter binding contracts.
//!
//! Called from:
//! - Function, method and closure body setup before the shared storage scan.
//!
//! Key details:
//! - An unannotated parameter stays open even when native inference specializes its ABI.
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
    /// Separates inferred ABI representation from a declared or open parameter contract.
    pub(crate) fn parameter(storage: &PhpType, declared: bool) -> Self {
        Self { storage: storage.clone(), contract: if declared { storage.clone() } else { PhpType::Mixed } }
    }

    /// Retains the capture evidence already used by whole-frame storage selection.
    pub(crate) fn capture(storage: &PhpType) -> Self {
        Self { storage: storage.clone(), contract: storage.clone() }
    }
}
