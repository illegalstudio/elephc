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
use std::collections::{BTreeMap, HashMap};
use std::ops::Bound::{Excluded, Unbounded};

/// Borrowed native addresses copied across the C ABI without inspecting their payloads.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    next_token: u64,
    epoch: u64,
}

impl Default for NativeReferenceTypeSources {
    /// Starts nonzero so zero can report an ABI failure without masquerading as a live epoch.
    fn default() -> Self {
        Self { bindings: HashMap::new(), sources: BTreeMap::new(), next_token: 1, epoch: 1 }
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
        if let Some(previous) = self.bindings.insert(binding, token) { self.sources.remove(&previous); }
        self.sources.insert(token, NativeReferenceTypeSource { token, binding, cell, validator });
        self.next_token = next_token;
        self.epoch = epoch;
        Some(token)
    }

    /// Retires only the selected physical property before its displaced cell can run destructors.
    fn unbind(&mut self, binding: usize) -> Option<()> {
        let Some(token) = self.bindings.get(&binding).copied() else { return Some(()); };
        let epoch = self.epoch.checked_add(1)?;
        self.bindings.remove(&binding);
        self.sources.remove(&token);
        self.epoch = epoch;
        Some(())
    }

    /// Copies the next live source in registration order without lending a map entry to native code.
    fn next(&self, after: u64) -> Option<NativeReferenceTypeSource> {
        self.sources.range((Excluded(after), Unbounded)).next().map(|(_, source)| *source)
    }

    /// Clears a retiring worker arena while keeping old tokens permanently invalid.
    fn reset(&mut self) -> Option<()> {
        let epoch = self.epoch.checked_add(1)?;
        self.bindings.clear();
        self.sources.clear();
        self.epoch = epoch;
        Some(())
    }
}

thread_local! {
    /// Source metadata follows the native worker; no runtime address crosses workers here.
    static SOURCES: RefCell<NativeReferenceTypeSources> = RefCell::new(NativeReferenceTypeSources::default());
}

/// Registers or replaces one property binding and returns its nonzero source token, or zero on failure.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_bind_v1(binding: usize, cell: usize, validator: usize) -> u64 {
    std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().bind(binding, cell, validator)))
        .ok().flatten().unwrap_or(0)
}

/// Removes one binding without removing another clone or property that shares the same cell.
#[no_mangle]
pub extern "C" fn __elephc_reference_sources_unbind_v1(binding: usize) -> i32 {
    match std::panic::catch_unwind(|| SOURCES.with(|sources| sources.borrow_mut().unbind(binding))) {
        Ok(Some(())) => 0,
        _ => -1,
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

/// Returns the nonzero mutation epoch, or zero on failure, for post-callback convergence checks.
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
}
