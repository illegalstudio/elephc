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
