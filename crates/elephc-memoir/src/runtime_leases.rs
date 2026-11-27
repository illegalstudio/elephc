//! Purpose:
//! Owns small runtime lease counters and detached-address lifetime registries.
//!
//! Called from:
//! - Magician's opaque ABI context ownership and process-global PCNTL registry.
//!
//! Key details:
//! - Policy about what references an address remains with the caller.
//! - Memoir owns counter overflow, last-lease detection, and detached-address reclamation mechanics.

use std::collections::{HashMap, HashSet};

/// A checked count of opaque owners keeping one runtime allocation alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeLeaseCounter {
    owners: usize,
}

impl RuntimeLeaseCounter {
    /// Creates a counter with the caller's initial owner count.
    pub const fn new(owners: usize) -> Self {
        Self { owners }
    }

    /// Adds one owner, returning false rather than wrapping at the representable limit.
    pub fn try_retain(&mut self) -> bool {
        let Some(owners) = self.owners.checked_add(1) else {
            return false;
        };
        self.owners = owners;
        true
    }

    /// Reports whether at least one owner keeps the allocation alive.
    pub const fn has_owners(&self) -> bool {
        self.owners != 0
    }

    /// Releases one owner and reports whether that was the final live lease.
    pub fn release(&mut self) -> bool {
        if self.owners == 0 {
            return false;
        }
        self.owners -= 1;
        self.owners == 0
    }
}

/// Active and detached leases for allocations named by stable nonzero addresses.
#[derive(Default)]
pub struct RuntimeAddressLeaseRegistry {
    detached: HashSet<usize>,
    active: HashMap<usize, usize>,
}

impl RuntimeAddressLeaseRegistry {
    /// Acquires one active-use lease, returning false for null addresses or counter overflow.
    pub fn try_acquire(&mut self, address: usize) -> bool {
        if address == 0 {
            return false;
        }
        let count = self.active.entry(address).or_default();
        let Some(next) = count.checked_add(1) else {
            return false;
        };
        *count = next;
        true
    }

    /// Marks an allocation detached when an external reference or active use still retains it.
    pub fn defer_if_referenced(&mut self, address: usize, externally_referenced: bool) -> bool {
        if address == 0 || !(externally_referenced || self.active.contains_key(&address)) {
            return false;
        }
        self.detached.insert(address);
        true
    }

    /// Claims a detached allocation only after external and active references have disappeared.
    pub fn take_if_unreferenced(&mut self, address: usize, externally_referenced: bool) -> bool {
        if address == 0 || externally_referenced || self.active.contains_key(&address) {
            return false;
        }
        self.detached.remove(&address)
    }

    /// Releases one active use and claims the detached allocation when it becomes unreferenced.
    pub fn release_and_take_if_unreferenced(
        &mut self,
        address: usize,
        externally_referenced: bool,
    ) -> bool {
        if address == 0 {
            return false;
        }
        let Some(count) = self.active.get_mut(&address) else {
            return false;
        };
        if *count > 1 {
            *count -= 1;
            return false;
        }
        self.active.remove(&address);
        if externally_referenced {
            return false;
        }
        self.detached.remove(&address)
    }

    /// Reports whether the address currently has an active-use lease.
    pub fn is_active(&self, address: usize) -> bool {
        self.active.contains_key(&address)
    }

    /// Reports whether teardown has detached the allocation for deferred reclamation.
    pub fn is_detached(&self, address: usize) -> bool {
        self.detached.contains(&address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The final owner release is reported once without underflowing.
    #[test]
    fn runtime_lease_counter_reports_only_the_final_release() {
        let mut leases = RuntimeLeaseCounter::new(1);
        assert!(leases.try_retain());
        assert!(!leases.release());
        assert!(leases.release());
        assert!(!leases.release());
    }

    /// Detached addresses become collectable after their last active lease and external reference.
    #[test]
    fn runtime_address_registry_claims_only_unreferenced_detached_addresses() {
        let mut leases = RuntimeAddressLeaseRegistry::default();
        assert!(leases.try_acquire(7));
        assert!(leases.defer_if_referenced(7, true));
        assert!(!leases.release_and_take_if_unreferenced(7, true));
        assert!(leases.is_detached(7));
        assert!(leases.take_if_unreferenced(7, false));
        assert!(!leases.is_detached(7));
    }
}
