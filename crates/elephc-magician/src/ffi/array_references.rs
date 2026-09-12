//! Purpose:
//! Invalidates eval array-reference metadata when its boxed runtime cell is freed.
//!
//! Called from:
//! - Context array-alias registration and the generated heap-free callback.
//!
//! Key details:
//! - The registry owns no cells or contexts. Shared validity tokens distinguish address reuse.
//! - Retirement changes validity flags and queues removals, never mutating a borrowed eval context.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

static ARRAY_REFERENCE_CELLS: OnceLock<Mutex<HashMap<usize, ArrayReferenceObservers>>> = OnceLock::new();

/// Weak observers for one allocation; neither the runtime cell nor an eval context is retained.
struct ArrayReferenceObservers {
    lifetime: Weak<AtomicBool>,
    retirements: Vec<Weak<Mutex<Vec<usize>>>>,
}

/// Deferred metadata removals drained by the owning context outside native release callbacks.
#[derive(Default)]
pub(crate) struct ArrayReferenceRetirements(Arc<Mutex<Vec<usize>>>);

impl ArrayReferenceRetirements {
    /// Takes the retired addresses without scanning unrelated live array-reference metadata.
    pub(crate) fn take(&self) -> Vec<usize> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }
}

/// Shared allocation validity attached to references recorded for one boxed array cell.
#[derive(Clone)]
pub(crate) struct ArrayReferenceCellLifetime(Arc<AtomicBool>);

impl ArrayReferenceCellLifetime {
    /// Reports whether the exact registered allocation still exists at its original address.
    pub(crate) fn is_live(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Returns the weak registry without retaining runtime allocations or their eval contexts.
fn array_reference_cells() -> &'static Mutex<HashMap<usize, ArrayReferenceObservers>> {
    ARRAY_REFERENCE_CELLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Shares the live allocation token, creating a distinct token after a freed address is reused.
pub(crate) fn register_array_reference_cell(
    cell: usize,
    retirements: &ArrayReferenceRetirements,
) -> ArrayReferenceCellLifetime {
    let mut cells = array_reference_cells().lock().unwrap_or_else(|error| error.into_inner());
    let existing = cells.get(&cell).and_then(|entry| entry.lifetime.upgrade())
        .filter(|lifetime| lifetime.load(Ordering::Acquire));
    let lifetime = existing.unwrap_or_else(|| {
        let lifetime = Arc::new(AtomicBool::new(true));
        cells.insert(cell, ArrayReferenceObservers {
            lifetime: Arc::downgrade(&lifetime), retirements: Vec::new(),
        });
        lifetime
    });
    let observers = &mut cells.get_mut(&cell).unwrap().retirements;
    let observer = Arc::downgrade(&retirements.0);
    if !observers.iter().any(|existing| existing.ptr_eq(&observer)) {
        observers.retain(|observer| observer.strong_count() != 0);
        observers.push(observer);
    }
    ArrayReferenceCellLifetime(lifetime)
}

/// Invalidates every context's metadata for a cell before the native allocator can reuse it.
pub(crate) fn retire_array_reference_cell(cell: usize) {
    let Some(cells) = ARRAY_REFERENCE_CELLS.get() else { return; };
    let entry = cells.lock().unwrap_or_else(|error| error.into_inner()).remove(&cell);
    if let Some(entry) = entry {
        let Some(lifetime) = entry.lifetime.upgrade() else { return; };
        lifetime.store(false, Ordering::Release);
        for retirements in entry.retirements.into_iter().filter_map(|queue| queue.upgrade()) {
            retirements.lock().unwrap_or_else(|error| error.into_inner()).push(cell);
        }
    }
}

/// Receives a validated dying Mixed cell address without letting a panic cross the C ABI.
#[cfg(not(test))]
pub(crate) extern "C" fn retire_array_reference_cell_callback(cell: usize) {
    let _ = std::panic::catch_unwind(|| retire_array_reference_cell(cell));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ElephcEvalContext, EvalArrayReferenceKey, EvalReferenceTarget};
    use crate::value::RuntimeCellHandle;

    /// Reusing one address creates a new allocation token and never revives old reference targets.
    #[test]
    fn retired_array_cell_cannot_inherit_reference_metadata() {
        let allocation = Box::new(0_u8);
        let address = (&*allocation as *const u8) as usize;
        let cell = RuntimeCellHandle::from_raw(address as *mut _);
        let key = EvalArrayReferenceKey::Int(0);
        let mut context = ElephcEvalContext::new();
        context.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        assert!(context.array_element_alias(cell, &key).is_some());
        retire_array_reference_cell(address);
        assert!(context.array_element_alias(cell, &key).is_none());
        let retirements = ArrayReferenceRetirements::default();
        let fresh_lifetime = register_array_reference_cell(address, &retirements);
        assert!(fresh_lifetime.is_live());
        assert!(context.array_element_alias(cell, &key).is_none());
        context.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        assert!(context.array_element_alias(cell, &key).is_some());
        retire_array_reference_cell(address);
        assert!(!fresh_lifetime.is_live());
        assert_eq!(retirements.take(), vec![address]);
        assert!(context.array_element_alias(cell, &key).is_none());
    }

    /// Independent contexts observe the same retirement without a callback mutating either context.
    #[test]
    fn array_cell_retirement_invalidates_all_contexts() {
        let allocation = Box::new(0_u8);
        let address = (&*allocation as *const u8) as usize;
        let cell = RuntimeCellHandle::from_raw(address as *mut _);
        let key = EvalArrayReferenceKey::Int(0);
        let mut first = ElephcEvalContext::new();
        let mut second = ElephcEvalContext::new();
        first.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        second.bind_array_element_alias(cell, key.clone(), EvalReferenceTarget::Cell { cell });
        retire_array_reference_cell(address);
        assert!(first.array_element_alias(cell, &key).is_none());
        assert!(second.array_element_alias(cell, &key).is_none());
    }
}
