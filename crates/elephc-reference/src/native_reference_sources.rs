//! Purpose:
//! Tracks weak native property type sources by physical binding and allocation generation.
//!
//! Called from:
//! - The versioned native reference metadata ABI used by generated binding lifecycle helpers.
//!
//! Key details:
//! - Records never own or dereference receivers, cells, values or validator code pointers.
//! - Distinct clone bindings remain distinct even when cell and validator addresses agree.
//! - Enumeration copies records before PHP re-entry; retired tokens cannot revive on address reuse.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Bound::{Excluded, Unbounded};

/// Borrowed native addresses copied across the C ABI without inspecting their payloads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeReferenceTypeSource {
    pub token: u64,
    pub binding: usize,
    pub cell: usize,
    pub validator: usize,
}

/// One worker's source identities; all mutations finish without running PHP or runtime cleanup.
struct NativeReferenceTypeSources {
    bindings: HashMap<usize, u64>,
    sources: BTreeMap<u64, NativeReferenceTypeSource>,
    cells: HashMap<usize, BTreeSet<u64>>,
    cell_references: HashMap<usize, usize>,
    references: HashMap<usize, BTreeSet<u64>>,
    next_token: u64,
    epoch: u64,
}

impl Default for NativeReferenceTypeSources {
    /// Starts nonzero so zero can report an ABI failure without masquerading as a live epoch.
    fn default() -> Self {
        Self { bindings: HashMap::new(), sources: BTreeMap::new(),
            cells: HashMap::new(), cell_references: HashMap::new(), references: HashMap::new(),
            next_token: 1, epoch: 1 }
    }
}

impl NativeReferenceTypeSources {
    /// Adds one physical binding, preserving its token only while the exact binding is unchanged.
    fn bind(&mut self, binding: usize, cell: usize, validator: usize) -> Option<u64> {
        if binding == 0 || cell == 0 || validator == 0 { return None; }
        if let Some(source) = self.bindings.get(&binding).and_then(|token| self.sources.get(token)) {
            if source.cell == cell && source.validator == validator { return Some(source.token); }
        }
        let token = self.next_token;
        let next_token = token.checked_add(1)?;
        let epoch = self.epoch.checked_add(1)?;
        let reference = self.cell_references.get(&cell).copied();
        if let Some(previous) = self.bindings.insert(binding, token) { self.remove_source(previous); }
        self.sources.insert(token, NativeReferenceTypeSource { token, binding, cell, validator });
        self.cells.entry(cell).or_default().insert(token);
        if let Some(reference) = reference {
            self.cell_references.insert(cell, reference);
            self.references.entry(reference).or_default().insert(token);
        }
        self.next_token = next_token;
        self.epoch = epoch;
        Some(token)
    }

    /// Moves an existing physical declaration to replacement storage without retaining its old cell.
    fn rebind(&mut self, binding: usize, cell: usize) -> Option<()> {
        if binding == 0 || cell == 0 { return None; }
        let validator = self.bindings.get(&binding).and_then(|token| self.sources.get(token))
            .map(|source| source.validator);
        match validator {
            Some(validator) => self.bind(binding, cell, validator).map(|_| ()),
            None => Some(()),
        }
    }

    /// Retires only the selected physical property before its displaced cell can run destructors.
    fn unbind(&mut self, binding: usize) -> Option<()> {
        let Some(token) = self.bindings.get(&binding).copied() else { return Some(()); };
        let epoch = self.epoch.checked_add(1)?;
        self.bindings.remove(&binding);
        self.remove_source(token);
        self.epoch = epoch;
        Some(())
    }

    /// Copies the next live source in registration order without lending a map entry to native code.
    fn next(&self, after: u64) -> Option<NativeReferenceTypeSource> {
        self.sources.range((Excluded(after), Unbounded)).next().map(|(_, source)| *source)
    }

    /// Removes one token from every weak index, dropping address metadata with the last source.
    fn remove_source(&mut self, token: u64) {
        let Some(source) = self.sources.remove(&token) else { return; };
        if let Some(reference) = self.cell_references.get(&source.cell).copied() {
            if let Some(tokens) = self.references.get_mut(&reference) {
                tokens.remove(&token);
                if tokens.is_empty() { self.references.remove(&reference); }
            }
        }
        if let Some(tokens) = self.cells.get_mut(&source.cell) {
            tokens.remove(&token);
            if tokens.is_empty() {
                self.cells.remove(&source.cell);
                self.cell_references.remove(&source.cell);
            }
        }
    }

    /// Records a native-authenticated physical transition without inspecting the cell or wrapper.
    fn set_reference(&mut self, cell: usize, reference: usize) -> Option<()> {
        if cell == 0 { return None; }
        let Some(tokens) = self.cells.get(&cell) else { return Some(()); };
        let old_reference = self.cell_references.get(&cell).copied();
        if old_reference.unwrap_or(0) == reference { return Some(()); }
        let epoch = self.epoch.checked_add(1)?;
        if let Some(old) = old_reference {
            if let Some(index) = self.references.get_mut(&old) {
                for token in tokens { index.remove(token); }
                if index.is_empty() { self.references.remove(&old); }
            }
        }
        if reference == 0 {
            self.cell_references.remove(&cell);
        } else {
            self.cell_references.insert(cell, reference);
            self.references.entry(reference).or_default().extend(tokens.iter().copied());
        }
        self.epoch = epoch;
        Some(())
    }

    /// Copies one matching record without scanning properties attached to other native cells.
    fn next_indexed(&self, index: Option<&BTreeSet<u64>>, after: u64) -> Option<NativeReferenceTypeSource> {
        let token = index?.range((Excluded(after), Unbounded)).next()?;
        self.sources.get(token).copied()
    }

    /// Clears a retiring worker arena while keeping old tokens permanently invalid.
    fn reset(&mut self) -> Option<()> {
        let epoch = self.epoch.checked_add(1)?;
        self.bindings.clear();
        self.sources.clear();
        self.cells.clear();
        self.cell_references.clear();
        self.references.clear();
        self.epoch = epoch;
        Some(())
    }
}

thread_local! {
    /// Source metadata follows the native worker; no runtime address crosses workers here.
    static SOURCES: RefCell<NativeReferenceTypeSources> = RefCell::new(NativeReferenceTypeSources::default());
}

/// Copies the indexed source set before a protected native coercion can re-enter PHP.
pub(super) fn snapshot_reference_sources(reference: usize) -> Vec<NativeReferenceTypeSource> {
    SOURCES.with(|sources| {
        let sources = sources.borrow();
        sources.references.get(&reference).into_iter().flatten()
            .filter_map(|token| sources.sources.get(token).copied()).collect()
    })
}

/// Registers or replaces one property binding and returns its nonzero source token, or zero on failure.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_bind_v1(binding: usize, cell: usize, validator: usize) -> u64 {
    std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().bind(binding, cell, validator)))
        .ok().flatten().unwrap_or(0)
}

/// Moves a registered property to its newly published cell, preserving the immutable declaration.
/// Unregistered storage remains unregistered and retains no native addresses.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_rebind_v1(binding: usize, cell: usize) -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().rebind(binding, cell))) {
        Ok(Some(())) => 0,
        _ => -1,
    }
}

/// Removes one binding without removing another clone or property that shares the same cell.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_unbind_v1(binding: usize) -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().unbind(binding))) {
        Ok(Some(())) => 0,
        _ => -1,
    }
}

/// Indexes a native-authenticated canonical wrapper, or clears it when `reference` is zero.
/// Unknown cells retain no address metadata; callers bind before reporting their current shape.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_set_reference_v1(cell: usize, reference: usize) -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().set_reference(cell, reference))) {
        Ok(Some(())) => 0,
        _ => -1,
    }
}

/// Copies the next source for an exact native cell without borrowing metadata across callbacks.
///
/// # Safety
/// `output` must point to writable storage for one complete `NativeReferenceTypeSource`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_sources_next_cell_v1(
    cell: usize, after: u64, output: *mut NativeReferenceTypeSource,
) -> i32 {
    unsafe { next_indexed_source(cell, after, output, false) }
}

/// Copies the next source attached to a canonical PHP reference through any native cell.
///
/// # Safety
/// `output` must point to writable storage for one complete `NativeReferenceTypeSource`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_sources_next_reference_v1(
    reference: usize, after: u64, output: *mut NativeReferenceTypeSource,
) -> i32 {
    unsafe { next_indexed_source(reference, after, output, true) }
}

/// Reads one weak index and ends its RefCell borrow before copying the result to native storage.
unsafe fn next_indexed_source(
    identity: usize, after: u64, output: *mut NativeReferenceTypeSource, reference_index: bool,
) -> i32 {
    if output.is_null() || identity == 0 { return -1; }
    match std::panic::catch_unwind(|| SOURCES.with(|sources| {
        let sources = sources.borrow();
        let index = if reference_index { sources.references.get(&identity) } else { sources.cells.get(&identity) };
        sources.next_indexed(index, after)
    })) {
        Ok(Some(source)) => { unsafe { output.write(source); } 1 }
        Ok(None) => 0,
        Err(_) => -1,
    }
}

/// Copies one source, returning one for a record, zero for exhaustion and minus one for failure.
///
/// # Safety
/// `output` must point to writable storage for one complete `NativeReferenceTypeSource`.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_sources_next_v1(
    after: u64, output: *mut NativeReferenceTypeSource,
) -> i32 {
    if output.is_null() { return -1; }
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow().next(after))) {
        Ok(Some(source)) => { unsafe { output.write(source); } 1 }
        Ok(None) => 0,
        Err(_) => -1,
    }
}

/// Reports whether a copied source still exists after a coercion callback re-enters PHP.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_contains_v1(token: u64) -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow().sources.contains_key(&token))) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(_) => -1,
    }
}

/// Returns the nonzero metadata epoch, or zero on failure, for post-callback convergence checks.
/// Callers must still recheck physical cell shape after PHP even when this epoch is unchanged.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_epoch_v1() -> u64 {
    std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow().epoch)).unwrap_or(0)
}

/// Invalidates all arena sources before native worker storage can be recycled.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_reset_v1() -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().reset())) {
        Ok(Some(())) => 0,
        _ => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All supported native targets use the same four-word source record layout.
    #[test]
    fn native_reference_source_record_matches_the_native_abi() {
        assert_eq!(std::mem::size_of::<NativeReferenceTypeSource>(), 32);
        assert_eq!(std::mem::align_of::<NativeReferenceTypeSource>(), 8);
        assert_eq!(std::mem::offset_of!(NativeReferenceTypeSource, token), 0);
        assert_eq!(std::mem::offset_of!(NativeReferenceTypeSource, binding), 8);
        assert_eq!(std::mem::offset_of!(NativeReferenceTypeSource, cell), 16);
        assert_eq!(std::mem::offset_of!(NativeReferenceTypeSource, validator), 24);
    }

    /// Clone properties contribute independent sources even when they share their physical cell.
    #[test]
    fn native_reference_sources_keep_each_physical_binding() {
        let mut sources = NativeReferenceTypeSources::default();
        let original = sources.bind(10, 100, 1000).unwrap();
        let cloned = sources.bind(20, 100, 1000).unwrap();
        assert_ne!(original, cloned);
        assert_eq!(sources.next(0).unwrap().token, original);
        assert_eq!(sources.next(original).unwrap().token, cloned);
        sources.unbind(10).unwrap();
        assert!(!sources.sources.contains_key(&original));
        assert_eq!(sources.next(0).unwrap().token, cloned);
        sources.unbind(20).unwrap();
        assert!(sources.next(0).is_none());
    }

    /// Unset replacement preserves the physical declaration while retiring only its old generation.
    #[test]
    fn native_reference_source_rebind_moves_only_the_selected_property() {
        let mut sources = NativeReferenceTypeSources::default();
        let first = sources.bind(10, 100, 1000).unwrap();
        let peer = sources.bind(20, 100, 2000).unwrap();
        sources.set_reference(100, 500).unwrap();
        sources.rebind(10, 200).unwrap();
        let replacement = sources.next_indexed(sources.cells.get(&200), 0).unwrap();
        assert!(replacement.token > peer && replacement.token != first);
        assert_eq!(replacement.validator, 1000);
        assert_eq!(sources.next_indexed(sources.references.get(&500), 0).unwrap().token, peer);
        assert_eq!(sources.references[&500].len(), 1);
        sources.rebind(20, 300).unwrap();
        assert!(!sources.cell_references.contains_key(&100));
        assert!(!sources.references.contains_key(&500));
    }

    /// Invalid, unknown and exhausted replacement attempts never erase the previous constraint.
    #[test]
    fn native_reference_source_rebind_preserves_constraints_on_failure() {
        let mut sources = NativeReferenceTypeSources::default();
        let token = sources.bind(10, 100, 1000).unwrap();
        assert_eq!(sources.rebind(999, 300), Some(()));
        assert!(!sources.cells.contains_key(&300));
        assert_eq!(sources.rebind(10, 0), None);
        sources.next_token = u64::MAX;
        assert_eq!(sources.rebind(10, 200), None);
        assert_eq!(sources.next(0).unwrap().token, token);
        assert_eq!(sources.next(0).unwrap().cell, 100);
    }

    /// Rebinding and arena reuse never reactivate a source copied before PHP re-entry.
    #[test]
    fn native_reference_source_tokens_follow_binding_generations() {
        let mut sources = NativeReferenceTypeSources::default();
        let first = sources.bind(10, 100, 1000).unwrap();
        let epoch = sources.epoch;
        assert_eq!(sources.bind(10, 100, 1000), Some(first));
        assert_eq!(sources.epoch, epoch);
        let rebound = sources.bind(10, 200, 1000).unwrap();
        assert!(rebound > first);
        assert!(!sources.sources.contains_key(&first));
        sources.reset().unwrap();
        let recycled = sources.bind(10, 100, 1000).unwrap();
        assert!(recycled > rebound);
        assert_eq!(sources.next(0).unwrap().token, recycled);
        assert!(!sources.sources.contains_key(&rebound));
    }

    /// A callback may delete pending sources and add another without retaining a stale map iterator.
    #[test]
    fn native_reference_source_enumeration_survives_reentrant_mutation() {
        let mut sources = NativeReferenceTypeSources::default();
        let first = sources.bind(10, 100, 1000).unwrap();
        let pending = sources.bind(20, 100, 2000).unwrap();
        let copied = sources.next(0).unwrap();
        sources.unbind(20).unwrap();
        let later = sources.bind(30, 100, 3000).unwrap();
        assert_eq!(copied.token, first);
        assert_eq!(sources.next(copied.token).unwrap().token, later);
        assert!(!sources.sources.contains_key(&pending));
        assert!(sources.next(u64::MAX).is_none());
    }

    /// Exhaustion and invalid addresses fail before replacing any live source.
    #[test]
    fn native_reference_source_allocation_failure_preserves_old_binding() {
        let mut sources = NativeReferenceTypeSources::default();
        let first = sources.bind(10, 100, 1000).unwrap();
        let epoch = sources.epoch;
        assert_eq!(sources.bind(0, 100, 1000), None);
        assert_eq!(sources.bind(10, 0, 1000), None);
        assert_eq!(sources.bind(10, 200, 0), None);
        sources.next_token = u64::MAX;
        assert_eq!(sources.bind(10, 200, 1000), None);
        assert_eq!(sources.epoch, epoch);
        assert_eq!(sources.next(0).unwrap().token, first);
    }

    /// Separate cells sharing a wrapper contribute all of their independent property constraints.
    #[test]
    fn native_reference_sources_index_shared_wrappers_across_cells() {
        let mut sources = NativeReferenceTypeSources::default();
        let first = sources.bind(10, 100, 1000).unwrap();
        let cloned = sources.bind(20, 100, 1000).unwrap();
        let other_cell = sources.bind(30, 200, 2000).unwrap();
        sources.bind(40, 300, 3000).unwrap();
        sources.set_reference(100, 500).unwrap();
        sources.set_reference(200, 500).unwrap();
        sources.set_reference(300, 600).unwrap();
        assert_eq!(sources.next_indexed(sources.cells.get(&100), 0).unwrap().token, first);
        assert_eq!(sources.next_indexed(sources.cells.get(&100), first).unwrap().token, cloned);
        assert!(sources.next_indexed(sources.cells.get(&100), cloned).is_none());
        assert_eq!(sources.references[&500], BTreeSet::from([first, cloned, other_cell]));
        sources.unbind(10).unwrap();
        assert_eq!(sources.references[&500], BTreeSet::from([cloned, other_cell]));
        sources.unbind(20).unwrap();
        assert!(!sources.cells.contains_key(&100));
        assert!(!sources.cell_references.contains_key(&100));
        assert_eq!(sources.references[&500], BTreeSet::from([other_cell]));
    }

    /// Cell retirement cannot leave a wrapper association that an allocator's reused address inherits.
    #[test]
    fn native_reference_source_indexes_do_not_revive_recycled_addresses() {
        let mut sources = NativeReferenceTypeSources::default();
        sources.set_reference(100, 500).unwrap();
        assert!(sources.cell_references.is_empty(), "unknown cells retain no weak addresses");
        let first = sources.bind(10, 100, 1000).unwrap();
        sources.set_reference(100, 500).unwrap();
        let changed_validator = sources.bind(10, 100, 2000).unwrap();
        assert!(changed_validator > first);
        assert_eq!(sources.references[&500], BTreeSet::from([changed_validator]));
        sources.unbind(10).unwrap();
        assert!(sources.references.is_empty());
        let recycled = sources.bind(10, 100, 1000).unwrap();
        assert!(recycled > changed_validator);
        assert!(sources.cell_references.is_empty());
        assert!(sources.next_indexed(sources.references.get(&500), 0).is_none());
        sources.set_reference(100, 500).unwrap();
        sources.reset().unwrap();
        assert!(sources.references.is_empty() && sources.cells.is_empty()
            && sources.cell_references.is_empty());
    }

    /// A callback can detach a pending source and attach an older unmatched source to this wrapper.
    #[test]
    fn native_reference_source_index_later_writes_restart_after_shape_changes() {
        let mut sources = NativeReferenceTypeSources::default();
        let initially_unmatched = sources.bind(10, 100, 1000).unwrap();
        let first_match = sources.bind(20, 200, 2000).unwrap();
        let removed_match = sources.bind(30, 300, 3000).unwrap();
        sources.set_reference(200, 500).unwrap();
        sources.set_reference(300, 500).unwrap();
        let copied = sources.next_indexed(sources.references.get(&500), 0).unwrap();
        assert_eq!(copied.token, first_match);
        let epoch = sources.epoch;
        sources.set_reference(100, 500).unwrap();
        sources.set_reference(300, 0).unwrap();
        assert!(sources.epoch > epoch);
        assert!(sources.sources.contains_key(&removed_match), "detaching a wrapper does not unset its property");
        assert!(sources.next_indexed(sources.references.get(&500), copied.token).is_none());
        assert_eq!(sources.next_indexed(sources.references.get(&500), 0).unwrap().token, initially_unmatched,
            "a later assignment starts at zero to include a lower token that joined after the previous snapshot");
    }

    /// Failed shape publication leaves both indexes and their convergence epoch intact.
    #[test]
    fn native_reference_source_index_failure_preserves_current_shape() {
        let mut sources = NativeReferenceTypeSources::default();
        let token = sources.bind(10, 100, 1000).unwrap();
        sources.set_reference(100, 500).unwrap();
        sources.epoch = u64::MAX;
        assert_eq!(sources.set_reference(100, 600), None);
        assert_eq!(sources.set_reference(100, 0), None);
        assert_eq!(sources.cell_references[&100], 500);
        assert_eq!(sources.references[&500], BTreeSet::from([token]));
        assert_eq!(sources.set_reference(0, 500), None);
        assert_eq!(sources.set_reference(100, 500), Some(()), "an unchanged shape needs no new epoch");
    }
}
