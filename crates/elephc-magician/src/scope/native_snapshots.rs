//! Purpose:
//! Retires native activation snapshots after their values have been written back.
//!
//! Called from:
//! - The native eval scope boundary after synchronized local publication.
//!
//! Key details:
//! - Interpreter-only bindings survive, including references retaining an imported value.
//! - Independent owners of an equal pointer remain independent retirement obligations.

use super::*;

impl ElephcEvalScope {
    /// Removes synchronized entries, transferring one lease to any surviving borrowed alias.
    pub(crate) fn retire_native_snapshots(&mut self) -> Vec<RuntimeCellHandle> {
        let mut names = std::mem::take(&mut self.aot_visible_names).into_iter().collect::<Vec<_>>();
        names.sort();
        let mut removed = Vec::new();
        for name in names {
            self.native_storage_types.remove(&name);
            self.native_binding_types.remove(&name);
            self.pending_native_sources.remove(&name);
            self.local_source_views.remove(&name);
            self.native_detached_names.remove(&name);
            self.reference_targets.remove(&name);
            if let Some(entry) = self.entries.remove(&name) { removed.push(entry); }
        }
        let mut owners = Vec::new();
        for entry in removed {
            let Some(cell) = owned_cell(Some(entry)) else { continue; };
            let alias = self.entries.values_mut().find(|alias| alias.flags.is_visible()
                && alias.flags.by_ref && alias.flags.ownership == ScopeCellOwnership::Borrowed
                && alias.cell == cell);
            if let Some(alias) = alias { alias.flags.ownership = ScopeCellOwnership::Owned; }
            else { owners.push(cell); }
        }
        self.bump_generation();
        owners
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native copies retire while dynamic locals and a borrowed alias retain their values.
    #[test]
    fn native_snapshot_retirement_preserves_dynamic_bindings_and_alias_owners() {
        let one = RuntimeCellHandle::from_raw(8usize as *mut crate::value::RuntimeCell);
        let two = RuntimeCellHandle::from_raw(16usize as *mut crate::value::RuntimeCell);
        let mut scope = ElephcEvalScope::new();
        scope.set_native_binding_type("first".into(), crate::eval_ir::EvalParameterType::new(
            vec![crate::eval_ir::EvalParameterTypeVariant::Int], false));
        scope.set_from_aot("first", one, ScopeCellOwnership::Owned);
        scope.set_from_aot("second", one, ScopeCellOwnership::Owned);
        scope.set_reference("dynamic_alias", "first", one, ScopeCellOwnership::Borrowed);
        scope.set("dynamic", two, ScopeCellOwnership::Owned);
        assert_eq!(scope.retire_native_snapshots(), vec![one]);
        assert!(!scope.contains_visible("first") && !scope.contains_visible("second"));
        assert_eq!(scope.visible_cell("dynamic_alias"), Some(one));
        assert_eq!(scope.entry("dynamic_alias").unwrap().flags.ownership, ScopeCellOwnership::Owned);
        assert_eq!(scope.visible_cell("dynamic"), Some(two));
        assert!(scope.aot_visible_cells().is_empty());
        assert!(scope.native_binding_type("first").is_none(), "retired native producers cannot constrain a later binding");
        assert!(scope.retire_native_snapshots().is_empty());
        let mut remaining = scope.drain_owned_cells();
        remaining.sort_by_key(|cell| cell.as_ptr() as usize);
        assert_eq!(remaining, vec![one, two]);
    }
}
