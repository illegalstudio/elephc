//! Purpose:
//! Unit tests for scope visibility, ownership, references, aliases, generations,
//! dirty markers, unset behavior, and draining.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Fake cell pointers are compared as opaque handles and never dereferenced.

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
