//! Purpose:
//! Stores only finite callable-name facts in one CFG local state.
//!
//! Called from:
//! - `crate::codegen::callable_reachability::analyze_value_names()`.
//!
//! Key details:
//! - An absent slot means unknown, including at exception-handler roots.
//! - Joins drop facts absent from either predecessor and union retained names.
//! - Unknown compiler temporaries never allocate per-block state entries.

use std::collections::BTreeMap;

use crate::ir::LocalSlotId;

use super::CallableNameSet;

/// Sparse local facts with the same unknown top element as the SSA lattice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CallableLocalState {
    local_count: usize,
    names: BTreeMap<LocalSlotId, CallableNameSet>,
}

impl CallableLocalState {
    /// Starts every declared slot at unknown without allocating a dense vector.
    pub(super) fn new(local_count: usize) -> Self {
        Self { local_count, names: BTreeMap::new() }
    }

    /// Returns a stored finite fact, leaving absent and invalid slots unknown.
    pub(super) fn get(&self, slot: LocalSlotId) -> Option<&CallableNameSet> {
        self.names.get(&slot)
    }

    /// Replaces a declared slot and removes entries that become unknown.
    pub(super) fn set(&mut self, slot: LocalSlotId, names: CallableNameSet) {
        if slot.as_raw() as usize >= self.local_count {
            return;
        }
        if matches!(names, CallableNameSet::Unknown) {
            self.names.remove(&slot);
        } else {
            self.names.insert(slot, names);
        }
    }

    /// Invalidates every finite local fact after opaque scope mutation.
    pub(super) fn clear(&mut self) {
        self.names.clear();
    }

    /// Joins incoming facts, treating missing slots on either side as unknown.
    pub(super) fn merge_from(&mut self, incoming: &Self) -> bool {
        let mut changed = false;
        self.names.retain(|slot, names| {
            let Some(other) = incoming.names.get(slot) else {
                changed = true;
                return false;
            };
            changed |= names.merge_from(other);
            !matches!(names, CallableNameSet::Unknown)
        });
        changed
    }
}
