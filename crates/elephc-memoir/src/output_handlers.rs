//! Purpose:
//! Stores retained output handlers until the native buffer detaches them.
//!
//! Called from:
//! - Magician's output-handler installation, invocation and release adapters.
//!
//! Key details:
//! - Each entry owns one callback lease and one foreign context lease.
//! - Removing an entry transfers both leases out of the lock.

use std::{collections::HashMap, sync::{Mutex, OnceLock}};

/// One registered eval output handler: the owning context and the retained
/// handler callable, both stored as raw addresses.
#[derive(Clone, Copy)]
pub struct OutputHandler {
    /// Address of one retained foreign context.
    pub context: usize,
    /// Address of the retained callable cell.
    pub callback: usize,
}

/// Active registrations and a monotonic identity counter independent of storage indices.
#[derive(Default)]
struct OutputHandlers {
    next: u64,
    entries: HashMap<u64, OutputHandler>,
}

impl OutputHandlers {
    /// Registers an owner with an identity representable by the native signed environment word.
    fn register(&mut self, entry: OutputHandler) -> Option<u64> {
        if self.next > i64::MAX as u64 { return None; }
        let id = self.next;
        self.next += 1;
        self.entries.insert(id, entry);
        Some(id)
    }

    /// Removes the entry if it matches the supplied context identity.
    fn take(&mut self, id: u64, context: Option<usize>) -> Option<OutputHandler> {
        if let Some(context) = context {
            if self.entries.get(&id)?.context != context {
                return None;
            }
        }
        self.entries.remove(&id)
    }
}

static OUTPUT_HANDLERS: OnceLock<Mutex<OutputHandlers>> = OnceLock::new();

/// Returns the process-local eval output-handler registry.
fn handlers() -> &'static Mutex<OutputHandlers> {
    OUTPUT_HANDLERS.get_or_init(|| Mutex::new(OutputHandlers::default()))
}

/// Publishes both incoming leases under a never-reused native handler identifier.
pub fn register(entry: OutputHandler) -> Option<u64> {
    handlers().lock().ok()?.register(entry)
}

/// Reads the current registered identities for one native callback invocation.
pub fn get(id: u64) -> Option<OutputHandler> {
    handlers().lock().ok()?.entries.get(&id).copied()
}

/// The registry lock was poisoned before an entry could be detached.
#[derive(Debug)]
pub struct RegistryPoisoned;

/// Detaches both leases before caller cleanup can run PHP.
pub fn take(id: u64, context: Option<usize>) -> Result<Option<OutputHandler>, RegistryPoisoned> {
    handlers().lock().map(|mut handlers| handlers.take(id, context)).map_err(|_| RegistryPoisoned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed registrations discard their storage without ever reusing a stale buffer identity.
    #[test]
    fn output_handler_registry_retires_entries_without_reusing_ids() {
        let mut handlers = OutputHandlers::default();
        for expected in 0..1024 {
            let id = handlers.register(OutputHandler { context: 1, callback: 2 }).unwrap();
            assert_eq!(id, expected);
            assert_eq!(handlers.entries.remove(&id).unwrap().callback, 2);
            assert!(handlers.entries.is_empty());
            assert!(!handlers.entries.contains_key(&id));
        }
        assert!(handlers.entries.capacity() < 16);
    }

    /// A failed start can only remove the entry owned by its original context.
    #[test]
    fn output_handler_registry_checks_original_context_before_removal() {
        let mut registry = OutputHandlers::default();
        let id = registry.register(OutputHandler { context: 1, callback: 2 }).unwrap();
        assert!(registry.take(id, Some(2)).is_none());
        assert_eq!(registry.take(id, Some(1)).unwrap().callback, 2);
        assert!(registry.take(id, None).is_none());
    }
}
