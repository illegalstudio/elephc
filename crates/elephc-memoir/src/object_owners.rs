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

/// Tracks the context to notify when a native identity is retired.
/// Neither the identity nor the registered context is kept alive by this table.
#[derive(Default)]
pub struct WeakIdentityOwners(HashMap<u64, usize>);

impl WeakIdentityOwners {
    /// Records the context of the current identity registration.
    pub fn register(&mut self, identity: u64, context: usize) {
        self.0.insert(identity, context);
    }

    /// Borrows the address of the current context without extending its lifetime.
    pub fn owner(&self, identity: u64) -> Option<usize> {
        self.0.get(&identity).copied()
    }

    /// Detaches the registered context before invoking its retirement hook.
    pub fn take(&mut self, identity: u64) -> Option<usize> {
        self.0.remove(&identity)
    }

    /// Clears a registration only if it still belongs to the calling context.
    pub fn clear_if_owned(&mut self, identity: u64, context: usize) {
        if let std::collections::hash_map::Entry::Occupied(entry) = self.0.entry(identity) {
            if *entry.get() == context {
                entry.remove();
            }
        }
    }

    /// Untracks a context before the caller reclaims its storage.
    pub fn untrack_context(&mut self, context: usize) {
        self.0.retain(|_, owner| *owner != context);
    }
}
