//! Purpose:
//! Tracks weak observers of native cell lifetime independently of PHP metadata.
//!
//! Called from:
//! - Magician's reference registrations and the native heap retirement callback.
//!
//! Key details:
//! - The registry retains neither native cells nor the observer queues.
//! - Retiring a cell invalidates its token before the allocator can reuse its address.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

static CELL_OBSERVERS: OnceLock<Mutex<HashMap<usize, CellObservers>>> = OnceLock::new();

/// Weak observers for one allocation; neither the runtime cell nor an eval context is retained.
struct CellObservers {
    lifetime: Weak<AtomicBool>,
    retirements: Vec<Weak<Mutex<Vec<usize>>>>,
}

/// Deferred metadata removals drained by the owning context outside native release callbacks.
#[derive(Default)]
pub struct CellRetirements(Arc<Mutex<Vec<usize>>>);

impl CellRetirements {
    /// Takes the retired addresses without scanning unrelated live metadata.
    pub fn take(&self) -> Vec<usize> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }
}

/// Shared validity for one exact native cell allocation.
#[derive(Clone)]
pub struct CellLifetime(Arc<AtomicBool>);

impl CellLifetime {
    /// Reports whether the exact registered allocation still exists at its original address.
    pub fn is_live(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// One metadata value bound to an exact native-cell allocation rather than its address alone.
struct WeakCellMetadataEntry<T> {
    value: T,
    lifetime: CellLifetime,
}

/// Non-owning metadata whose entries disappear when their exact cell allocation retires.
pub struct WeakCellMetadata<T> {
    entries: HashMap<usize, WeakCellMetadataEntry<T>>,
    retirements: CellRetirements,
}

impl<T> Default for WeakCellMetadata<T> {
    /// Creates an empty metadata table with its own deferred retirement queue.
    fn default() -> Self {
        Self { entries: HashMap::new(), retirements: CellRetirements::default() }
    }
}

impl<T> WeakCellMetadata<T> {
    /// Removes metadata invalidated before native storage reuse.
    fn prune(&mut self) {
        for address in self.retirements.take() {
            let _ = self.entries.remove(&address);
        }
    }

    /// Publishes metadata for the exact allocation currently occupying `address`.
    pub fn insert(&mut self, address: usize, value: T) -> Option<T> {
        self.prune();
        let lifetime = observe(address, &self.retirements);
        self.entries
            .insert(address, WeakCellMetadataEntry { value, lifetime })
            .map(|entry| entry.value)
    }

    /// Removes one address-bound value without retaining or retiring the native cell.
    pub fn remove(&mut self, address: usize) -> Option<T> {
        self.prune();
        self.entries.remove(&address).map(|entry| entry.value)
    }

    /// Removes every non-owning value while leaving native-cell lifetimes untouched.
    pub fn clear(&mut self) {
        self.entries.clear();
        let _ = self.retirements.take();
    }
}

impl<T: Copy> WeakCellMetadata<T> {
    /// Returns metadata only while the exact observed allocation remains live.
    pub fn get(&self, address: usize) -> Option<T> {
        self.entries.get(&address)
            .filter(|entry| entry.lifetime.is_live())
            .map(|entry| entry.value)
    }

    /// Copies live metadata to another exact allocation, replacing its previous value.
    pub fn copy(&mut self, source: usize, target: usize) -> Option<T> {
        let value = self.get(source);
        let _ = self.remove(target);
        if let Some(value) = value {
            let _ = self.insert(target, value);
        }
        value
    }
}

/// Returns the weak registry without retaining runtime allocations or their eval contexts.
fn cell_observers() -> &'static Mutex<HashMap<usize, CellObservers>> {
    CELL_OBSERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Shares the live allocation token, creating a distinct token after a freed address is reused.
pub fn observe(
    cell: usize,
    retirements: &CellRetirements,
) -> CellLifetime {
    let mut cells = cell_observers().lock().unwrap_or_else(|error| error.into_inner());
    let existing = cells.get(&cell).and_then(|entry| entry.lifetime.upgrade())
        .filter(|lifetime| lifetime.load(Ordering::Acquire));
    let lifetime = existing.unwrap_or_else(|| {
        let lifetime = Arc::new(AtomicBool::new(true));
        cells.insert(cell, CellObservers {
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
    CellLifetime(lifetime)
}

/// Invalidates every observer's metadata for a cell before the native allocator can reuse it.
pub fn retire(cell: usize) {
    let Some(cells) = CELL_OBSERVERS.get() else { return; };
    let entry = cells.lock().unwrap_or_else(|error| error.into_inner()).remove(&cell);
    if let Some(entry) = entry {
        let Some(lifetime) = entry.lifetime.upgrade() else { return; };
        lifetime.store(false, Ordering::Release);
        for retirements in entry.retirements.into_iter().filter_map(|queue| queue.upgrade()) {
            retirements.lock().unwrap_or_else(|error| error.into_inner()).push(cell);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Address reuse receives a fresh lifetime and cannot inherit retired metadata.
    #[test]
    fn weak_metadata_rejects_values_from_a_retired_allocation() {
        let address = usize::MAX - 91;
        let mut metadata = WeakCellMetadata::default();
        assert_eq!(metadata.insert(address, 7), None);
        assert_eq!(metadata.get(address), Some(7));

        retire(address);
        assert_eq!(metadata.get(address), None);
        assert_eq!(metadata.insert(address, 9), None);
        assert_eq!(metadata.get(address), Some(9));
        retire(address);
    }

    /// Copying metadata observes the target allocation independently of its source.
    #[test]
    fn weak_metadata_copy_tracks_the_target_lifetime() {
        let source = usize::MAX - 101;
        let target = usize::MAX - 102;
        let mut metadata = WeakCellMetadata::default();
        let _ = metadata.insert(source, 3);
        assert_eq!(metadata.copy(source, target), Some(3));
        retire(source);
        assert_eq!(metadata.get(source), None);
        assert_eq!(metadata.get(target), Some(3));
        retire(target);
    }
}
