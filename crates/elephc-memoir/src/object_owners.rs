//! Purpose:
//! Stores retained child cells that follow the lifetime of a native object.
//!
//! Called from:
//! - Magician closure construction and the native GC/free bridge hooks.
//!
//! Key details:
//! - Replacement and removal transfer owners out of the lock for explicit release.
//! - Cell addresses do not keep an eval context live.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static OBJECT_OWNERS: OnceLock<Mutex<HashMap<u64, Vec<usize>>>> = OnceLock::new();

/// Returns the process-local registry of independently retained child cells.
fn owners() -> &'static Mutex<HashMap<u64, Vec<usize>>> {
    OBJECT_OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Publishes already-retained children, transferring displaced owners to the caller.
///
/// If publication fails, returns the input owners instead so the caller can retire them.
pub fn replace(identity: u64, retained: Vec<usize>) -> Result<Option<Vec<usize>>, Vec<usize>> {
    match owners().lock() {
        Ok(mut owners) => Ok(owners.insert(identity, retained)),
        Err(_) => Err(retained),
    }
}

/// Borrows one GC edge from a live object without changing its refcount.
pub fn child(identity: u64, index: usize) -> Option<usize> {
    owners().lock().ok()?.get(&identity)?.get(index).copied()
}

/// Detaches the object's children so any recursive retirement runs outside the lock.
pub fn take(identity: u64) -> Option<Vec<usize>> {
    owners().lock().ok()?.remove(&identity)
}
