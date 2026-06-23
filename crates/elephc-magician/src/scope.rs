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

use crate::context::EvalReferenceTarget;
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

    /// Builds flags for a present runtime cell shared by PHP references.
    pub const fn reference(ownership: ScopeCellOwnership) -> Self {
        Self {
            present: true,
            unset: false,
            dirty: true,
            by_ref: true,
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

    /// Creates a present entry that participates in a PHP reference alias set.
    pub const fn reference(
        cell: RuntimeCellHandle,
        ownership: ScopeCellOwnership,
        generation: u64,
    ) -> Self {
        Self {
            cell,
            flags: ScopeEntryFlags::reference(ownership),
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
    global_aliases: HashMap<String, String>,
    reference_targets: HashMap<String, EvalReferenceTarget>,
    generation: u64,
}

impl ElephcEvalScope {
    /// Creates an empty materialized activation scope.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            global_aliases: HashMap::new(),
            reference_targets: HashMap::new(),
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

    /// Stores a variable while preserving existing PHP reference aliases.
    pub fn set_respecting_references(
        &mut self,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
        ownership: ScopeCellOwnership,
    ) -> Vec<RuntimeCellHandle> {
        let name = name.into();
        let Some(entry) = self.entries.get(&name).copied() else {
            return self.set(name, cell, ownership).into_iter().collect();
        };
        if !entry.flags().is_visible() || !entry.flags().by_ref {
            return self.set(name, cell, ownership).into_iter().collect();
        }

        self.bump_generation();
        let old_cell = entry.cell();
        let mut replaced = Vec::new();
        for (entry_name, entry) in &mut self.entries {
            let flags = entry.flags();
            if !flags.is_visible() || !flags.by_ref || entry.cell() != old_cell {
                continue;
            }
            if flags.ownership == ScopeCellOwnership::Owned
                && old_cell != cell
                && !replaced.contains(&old_cell)
            {
                replaced.push(old_cell);
            }
            let next_ownership = if entry_name == &name {
                ownership
            } else {
                ScopeCellOwnership::Borrowed
            };
            *entry = ScopeEntry::reference(cell, next_ownership, self.generation);
        }
        replaced
    }

    /// Binds a target variable name as a PHP reference to a source variable name.
    pub fn set_reference(
        &mut self,
        target: impl Into<String>,
        source: impl Into<String>,
        default_cell: RuntimeCellHandle,
        default_ownership: ScopeCellOwnership,
    ) -> Vec<RuntimeCellHandle> {
        let target = target.into();
        let source = source.into();
        self.bump_generation();
        let source_entry = self
            .entries
            .get(&source)
            .copied()
            .filter(|entry| entry.flags().is_visible());
        let (cell, source_ownership) = source_entry
            .map_or((default_cell, default_ownership), |entry| {
                (entry.cell(), entry.flags().ownership)
            });
        if target == source {
            let previous = self.entries.insert(
                source,
                ScopeEntry::reference(cell, source_ownership, self.generation),
            );
            return owned_cell_except(previous, cell).into_iter().collect();
        }

        let previous_source = self.entries.insert(
            source,
            ScopeEntry::reference(cell, source_ownership, self.generation),
        );
        let previous_target = self.entries.insert(
            target,
            ScopeEntry::reference(cell, ScopeCellOwnership::Borrowed, self.generation),
        );
        owned_cells_except([previous_source, previous_target], cell)
    }

    /// Records the caller-side storage target for one by-reference local variable.
    pub fn set_reference_target(&mut self, name: impl Into<String>, target: EvalReferenceTarget) {
        self.reference_targets.insert(name.into(), target);
    }

    /// Returns the caller-side storage target associated with one by-reference local.
    pub fn reference_target(&self, name: &str) -> Option<&EvalReferenceTarget> {
        self.reference_targets.get(name)
    }

    /// Marks a named variable as unset while preserving the fact that eval touched it.
    pub fn unset(&mut self, name: impl Into<String>) -> Option<RuntimeCellHandle> {
        self.bump_generation();
        let name = name.into();
        self.reference_targets.remove(&name);
        let previous = self
            .entries
            .insert(name, ScopeEntry::unset(self.generation));
        owned_cell(previous)
    }

    /// Marks a variable as unset while preserving ownership for remaining references.
    pub fn unset_respecting_references(
        &mut self,
        name: impl Into<String>,
    ) -> Option<RuntimeCellHandle> {
        let name = name.into();
        let Some(entry) = self.entries.get(&name).copied() else {
            return self.unset(name);
        };
        if !entry.flags().is_visible() || !entry.flags().by_ref {
            return self.unset(name);
        }
        let old_cell = entry.cell();
        let should_transfer_ownership = entry.flags().ownership == ScopeCellOwnership::Owned;
        self.bump_generation();
        let mut transferred = false;
        if should_transfer_ownership {
            for (entry_name, entry) in &mut self.entries {
                let flags = entry.flags();
                if entry_name == &name
                    || !flags.is_visible()
                    || !flags.by_ref
                    || entry.cell() != old_cell
                {
                    continue;
                }
                *entry =
                    ScopeEntry::reference(old_cell, ScopeCellOwnership::Owned, self.generation);
                transferred = true;
                break;
            }
        }
        self.reference_targets.remove(&name);
        let previous = self
            .entries
            .insert(name, ScopeEntry::unset(self.generation));
        if transferred {
            None
        } else {
            owned_cell(previous)
        }
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

    /// Marks a variable name as an alias to the eval context's global scope.
    pub fn mark_global_alias(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.mark_global_alias_to(name.clone(), name);
    }

    /// Marks a variable name as an alias to a differently named global variable.
    pub fn mark_global_alias_to(
        &mut self,
        name: impl Into<String>,
        global_name: impl Into<String>,
    ) {
        self.global_aliases.insert(name.into(), global_name.into());
    }

    /// Removes a variable's global alias marker after local `unset()`.
    pub fn clear_global_alias(&mut self, name: &str) {
        self.global_aliases.remove(name);
    }

    /// Returns true when the variable should resolve through the global scope.
    pub fn is_global_alias(&self, name: &str) -> bool {
        self.global_aliases.contains_key(name)
    }

    /// Returns the target global name for a local alias.
    pub fn global_alias_target(&self, name: &str) -> Option<&str> {
        self.global_aliases.get(name).map(String::as_str)
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
        self.reference_targets.clear();
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

/// Returns owned cells from replaced entries unless they match the replacement.
fn owned_cells_except(
    entries: impl IntoIterator<Item = Option<ScopeEntry>>,
    replacement: RuntimeCellHandle,
) -> Vec<RuntimeCellHandle> {
    let mut cells = Vec::new();
    for entry in entries {
        let Some(cell) = owned_cell_except(entry, replacement) else {
            continue;
        };
        if !cells.contains(&cell) {
            cells.push(cell);
        }
    }
    cells
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

    /// Verifies reference binding points two variable names at one runtime cell.
    #[test]
    fn set_reference_binds_names_to_source_cell() {
        let mut scope = ElephcEvalScope::new();
        let cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);

        scope.set("source", cell, ScopeCellOwnership::Owned);
        let replaced = scope.set_reference(
            "alias",
            "source",
            RuntimeCellHandle::from_raw(std::ptr::null_mut()),
            ScopeCellOwnership::Owned,
        );

        let source = scope.entry("source").expect("source entry should exist");
        let alias = scope.entry("alias").expect("alias entry should exist");
        assert!(replaced.is_empty());
        assert_eq!(source.cell(), cell);
        assert_eq!(alias.cell(), cell);
        assert!(source.flags().by_ref);
        assert!(alias.flags().by_ref);
        assert_eq!(source.flags().ownership, ScopeCellOwnership::Owned);
        assert_eq!(alias.flags().ownership, ScopeCellOwnership::Borrowed);
    }

    /// Verifies writing through one reference alias updates every alias in the group.
    #[test]
    fn set_respecting_references_updates_alias_group() {
        let mut scope = ElephcEvalScope::new();
        let old_cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        let new_cell = RuntimeCellHandle::from_raw(2usize as *mut crate::value::RuntimeCell);
        scope.set("source", old_cell, ScopeCellOwnership::Owned);
        scope.set_reference(
            "alias",
            "source",
            RuntimeCellHandle::from_raw(std::ptr::null_mut()),
            ScopeCellOwnership::Owned,
        );

        let replaced =
            scope.set_respecting_references("alias", new_cell, ScopeCellOwnership::Owned);

        assert_eq!(replaced, vec![old_cell]);
        assert_eq!(scope.visible_cell("source"), Some(new_cell));
        assert_eq!(scope.visible_cell("alias"), Some(new_cell));
        assert_eq!(
            scope.entry("alias").expect("alias").flags().ownership,
            ScopeCellOwnership::Owned
        );
        assert_eq!(
            scope.entry("source").expect("source").flags().ownership,
            ScopeCellOwnership::Borrowed
        );
    }

    /// Verifies unsetting an owned alias transfers ownership to a remaining reference.
    #[test]
    fn unset_respecting_references_transfers_owned_cell() {
        let mut scope = ElephcEvalScope::new();
        let cell = RuntimeCellHandle::from_raw(1usize as *mut crate::value::RuntimeCell);
        scope.set("source", cell, ScopeCellOwnership::Owned);
        scope.set_reference(
            "alias",
            "source",
            RuntimeCellHandle::from_raw(std::ptr::null_mut()),
            ScopeCellOwnership::Owned,
        );
        scope.set_respecting_references("alias", cell, ScopeCellOwnership::Owned);

        let replaced = scope.unset_respecting_references("alias");

        assert_eq!(replaced, None);
        assert!(scope.entry("alias").expect("alias").flags().unset);
        assert_eq!(
            scope.entry("source").expect("source").flags().ownership,
            ScopeCellOwnership::Owned
        );
    }

    /// Verifies local aliases can point at differently named globals.
    #[test]
    fn global_alias_to_records_target_name() {
        let mut scope = ElephcEvalScope::new();

        scope.mark_global_alias_to("alias", "source");

        assert!(scope.is_global_alias("alias"));
        assert_eq!(scope.global_alias_target("alias"), Some("source"));
        assert_eq!(scope.global_alias_target("source"), None);
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
