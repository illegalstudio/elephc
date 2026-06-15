//! Purpose:
//! Owns the materialized activation scope used by runtime eval.
//! The scope maps PHP variable names to opaque elephc runtime cells and tracks
//! unset/dirty/generation metadata for native reloads after an eval barrier.
//!
//! Called from:
//! - `crate::abi`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - Scope entries store runtime-cell handles only; the eval bridge does not
//!   introduce a second PHP value representation.

use std::collections::HashMap;

use crate::value::RuntimeCellHandle;

/// Records whether a scope entry owns or borrows its runtime cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeCellOwnership {
    Borrowed,
    Owned,
}

/// Tracks the observable PHP state associated with one named scope cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeEntryFlags {
    pub present: bool,
    pub unset: bool,
    pub dirty: bool,
    pub by_ref: bool,
    pub ownership: ScopeCellOwnership,
}

impl ScopeEntryFlags {
    /// Builds flags for a present runtime cell written by native code or eval.
    pub const fn present(ownership: ScopeCellOwnership) -> Self {
        Self {
            present: true,
            unset: false,
            dirty: true,
            by_ref: false,
            ownership,
        }
    }

    /// Builds flags for a PHP variable that has been unset from the dynamic scope.
    pub const fn unset() -> Self {
        Self {
            present: false,
            unset: true,
            dirty: true,
            by_ref: false,
            ownership: ScopeCellOwnership::Borrowed,
        }
    }

    /// Returns true when this entry names a PHP variable visible to reads.
    pub const fn is_visible(self) -> bool {
        self.present && !self.unset
    }
}

/// Stores one named variable's runtime cell plus sync metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeEntry {
    cell: RuntimeCellHandle,
    flags: ScopeEntryFlags,
    generation: u64,
}

impl ScopeEntry {
    /// Creates an entry for a present runtime cell at the given scope generation.
    pub const fn present(
        cell: RuntimeCellHandle,
        ownership: ScopeCellOwnership,
        generation: u64,
    ) -> Self {
        Self {
            cell,
            flags: ScopeEntryFlags::present(ownership),
            generation,
        }
    }

    /// Creates an unset marker at the given scope generation.
    pub const fn unset(generation: u64) -> Self {
        Self {
            cell: RuntimeCellHandle::from_raw(std::ptr::null_mut()),
            flags: ScopeEntryFlags::unset(),
            generation,
        }
    }

    /// Returns the runtime cell handle stored for this variable.
    pub const fn cell(self) -> RuntimeCellHandle {
        self.cell
    }

    /// Returns the PHP visibility and ownership flags for this entry.
    pub const fn flags(self) -> ScopeEntryFlags {
        self.flags
    }

    /// Returns the scope generation when this entry was last updated.
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// Clears the dirty flag after native code has synchronized the entry.
    pub fn mark_clean(&mut self) {
        self.flags.dirty = false;
    }
}

/// Materialized activation scope passed opaquely across the eval ABI.
pub struct ElephcEvalScope {
    entries: HashMap<String, ScopeEntry>,
    generation: u64,
}

impl ElephcEvalScope {
    /// Creates an empty materialized activation scope.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
        }
    }

    /// Returns the current scope generation counter.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Stores or replaces a named variable with a runtime cell handle.
    pub fn set(
        &mut self,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
        ownership: ScopeCellOwnership,
    ) -> Option<RuntimeCellHandle> {
        self.bump_generation();
        let previous = self.entries.insert(
            name.into(),
            ScopeEntry::present(cell, ownership, self.generation),
        );
        owned_cell_except(previous, cell)
    }

    /// Marks a named variable as unset while preserving the fact that eval touched it.
    pub fn unset(&mut self, name: impl Into<String>) -> Option<RuntimeCellHandle> {
        self.bump_generation();
        let previous = self
            .entries
            .insert(name.into(), ScopeEntry::unset(self.generation));
        owned_cell(previous)
    }

    /// Returns the entry for a named variable, including unset markers.
    pub fn entry(&self, name: &str) -> Option<ScopeEntry> {
        self.entries.get(name).copied()
    }

    /// Returns the visible runtime cell for a named variable, excluding unset markers.
    pub fn visible_cell(&self, name: &str) -> Option<RuntimeCellHandle> {
        self.entry(name)
            .filter(|entry| entry.flags().is_visible())
            .map(ScopeEntry::cell)
    }

    /// Returns true when the scope contains a visible value for the named variable.
    pub fn contains_visible(&self, name: &str) -> bool {
        self.visible_cell(name).is_some()
    }

    /// Returns the names of entries dirtied since the last synchronization.
    pub fn dirty_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|(name, entry)| entry.flags().dirty.then_some(name.as_str()))
            .collect()
    }

    /// Clears dirty flags after native code reloads or invalidates affected locals.
    pub fn mark_all_clean(&mut self) {
        for entry in self.entries.values_mut() {
            entry.mark_clean();
        }
    }

    /// Removes every entry and returns runtime cells owned by the scope.
    pub fn drain_owned_cells(&mut self) -> Vec<RuntimeCellHandle> {
        self.entries
            .drain()
            .filter_map(|(_, entry)| owned_cell(Some(entry)))
            .collect()
    }

    /// Advances the generation counter for a scope mutation.
    fn bump_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }
}

/// Returns the owned cell for a visible owned entry, if one exists.
fn owned_cell(entry: Option<ScopeEntry>) -> Option<RuntimeCellHandle> {
    let entry = entry?;
    let flags = entry.flags();
    (flags.is_visible() && flags.ownership == ScopeCellOwnership::Owned).then_some(entry.cell())
}

/// Returns an owned cell unless it is the same handle as the newly stored cell.
fn owned_cell_except(
    entry: Option<ScopeEntry>,
    replacement: RuntimeCellHandle,
) -> Option<RuntimeCellHandle> {
    owned_cell(entry).filter(|cell| *cell != replacement)
}

impl Default for ElephcEvalScope {
    /// Creates the default empty materialized activation scope.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies setting a variable records a visible dirty runtime-cell handle.
    #[test]
    fn set_records_visible_dirty_cell() {
        let mut scope = ElephcEvalScope::new();
        let cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        let old = scope.set("x", cell, ScopeCellOwnership::Borrowed);

        let entry = scope.entry("x").expect("entry must exist");
        assert_eq!(old, None);
        assert_eq!(entry.cell(), cell);
        assert!(entry.flags().is_visible());
        assert!(entry.flags().dirty);
        assert_eq!(entry.generation(), 1);
        assert_eq!(scope.visible_cell("x"), Some(cell));
    }

    /// Verifies unsetting a variable creates a dirty marker that is not visible.
    #[test]
    fn unset_records_missing_dirty_marker() {
        let mut scope = ElephcEvalScope::new();
        let cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        scope.set("x", cell, ScopeCellOwnership::Borrowed);
        scope.mark_all_clean();
        let old = scope.unset("x");

        let entry = scope.entry("x").expect("unset marker must exist");
        assert_eq!(old, None);
        assert!(entry.flags().unset);
        assert!(!entry.flags().is_visible());
        assert!(entry.flags().dirty);
        assert_eq!(scope.visible_cell("x"), None);
        assert_eq!(scope.dirty_names(), vec!["x"]);
    }

    /// Verifies replacing an owned entry returns the old cell for release.
    #[test]
    fn set_returns_replaced_owned_cell() {
        let mut scope = ElephcEvalScope::new();
        let old_cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        let new_cell = RuntimeCellHandle::from_raw(2usize as *mut crate::value::RuntimeCell);

        scope.set("x", old_cell, ScopeCellOwnership::Owned);
        let replaced = scope.set("x", new_cell, ScopeCellOwnership::Owned);

        assert_eq!(replaced, Some(old_cell));
        assert_eq!(scope.visible_cell("x"), Some(new_cell));
    }

    /// Verifies replacing an owned entry with the same cell does not release it.
    #[test]
    fn set_does_not_return_same_owned_cell() {
        let mut scope = ElephcEvalScope::new();
        let cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);

        scope.set("x", cell, ScopeCellOwnership::Owned);
        let replaced = scope.set("x", cell, ScopeCellOwnership::Owned);

        assert_eq!(replaced, None);
    }

    /// Verifies draining a scope returns only visible owned cells.
    #[test]
    fn drain_owned_cells_returns_visible_owned_entries() {
        let mut scope = ElephcEvalScope::new();
        let owned = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        let borrowed = RuntimeCellHandle::from_raw(2usize as *mut crate::value::RuntimeCell);
        scope.set("owned", owned, ScopeCellOwnership::Owned);
        scope.set("borrowed", borrowed, ScopeCellOwnership::Borrowed);
        scope.unset("borrowed");

        let drained = scope.drain_owned_cells();

        assert_eq!(drained, vec![owned]);
        assert!(scope.entry("owned").is_none());
        assert!(scope.entry("borrowed").is_none());
    }
}
