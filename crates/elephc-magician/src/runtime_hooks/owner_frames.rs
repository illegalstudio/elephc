//! Purpose:
//! Publishes Rust-owned temporary cells as edges of a suspended native activation.
//!
//! Called from:
//! - Runtime method and constructor bridges while their argument array remains owned by Rust.
//! - Interpreter argument invocation boundaries and their explicit source-owner cleanup.
//!
//! Key details:
//! - The activation record is pinned on the current coroutine stack, never allocated on the heap.
//! - The visitor borrows the actual pointer field; taking a cell clears it before PHP cleanup.
//! - Native exception boundaries return through Rust before this record is detached.

use crate::value::{RuntimeCell, RuntimeCellHandle};
pub(crate) use elephc_memoir::runtime_owners::{RuntimeOwnerSlot, RuntimeOwners, StagedRuntimeOwners};
#[cfg(not(test))]
use std::ffi::c_void;
#[cfg(not(test))]
use elephc_memoir::runtime_owner_frame::{
    OwnerFrameHooks, with_mutable_owner_frame, with_owner_frame,
};

/// Keeps an immobile Rust inventory visible through native calls and explicit slot cleanup.
pub(crate) fn with_runtime_owners<T: RuntimeOwners, R>(owners: &T, consume: impl FnOnce() -> R) -> R {
    #[cfg(not(test))]
    {
        // These hooks implement Memoir's activation layout on the current Fiber.
        unsafe { with_owner_frame(owners, OwnerFrameHooks {
            enter: __elephc_eval_owner_frame_enter_v1,
            leave: __elephc_eval_owner_frame_leave_v1,
            visit_slot: __elephc_eval_owner_frame_slot_v1,
        }, consume) }
    }
    #[cfg(test)]
    {
        let _ = owners;
        consume()
    }
}

/// Publishes the current shape of an inventory that grows or drains between runtime calls.
pub(crate) fn with_mutable_runtime_owners<T: RuntimeOwners, R>(
    owners: &mut T,
    consume: impl FnOnce(&mut T) -> R,
) -> R {
    #[cfg(not(test))]
    {
        // The owner frame visitor runs only while this worker is suspended in native code.
        unsafe {
            with_mutable_owner_frame(
                owners,
                OwnerFrameHooks {
                    enter: __elephc_eval_owner_frame_enter_v1,
                    leave: __elephc_eval_owner_frame_leave_v1,
                    visit_slot: __elephc_eval_owner_frame_slot_v1,
                },
                consume,
            )
        }
    }
    #[cfg(test)]
    {
        consume(owners)
    }
}

/// A method scope can change during execution but is frozen while its Fiber is inspected.
struct RuntimeScopeOwners(*const crate::scope::ElephcEvalScope);

impl RuntimeOwners for RuntimeScopeOwners {
    /// Borrows current entries only during the collector's pure suspended-stack callback.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        // The scoped call keeps this address live. No PHP or scope mutation runs during a visit.
        unsafe { &*self.0 }.visit_owned_cell_slots(visit);
    }
}

/// Publishes an interpreted method's actual scope while its body can enter native suspension.
pub(crate) fn with_runtime_scope_owners<R>(
    scope: &mut crate::scope::ElephcEvalScope,
    consume: impl FnOnce(&mut crate::scope::ElephcEvalScope) -> R,
) -> R {
    let owners = RuntimeScopeOwners(std::ptr::from_mut(scope));
    with_runtime_owners(&owners, || {
        // Execution and collection alternate on one worker. The visitor never runs concurrently
        // with this mutable scope access, and no borrowed entry survives the pure callback.
        consume(unsafe { &mut *owners.0.cast_mut() })
    })
}

/// Adopts one temporary owner until the consumer explicitly takes it for cleanup.
pub(super) fn with_owned_runtime_cell<R>(
    value: RuntimeCellHandle, consume: impl FnOnce(&RuntimeOwnerSlot) -> R,
) -> R {
    debug_assert!(!value.is_borrowed());
    let owner = RuntimeOwnerSlot::new(value);
    let result = with_runtime_owners(&owner, || consume(&owner));
    debug_assert!(owner.is_empty(), "the runtime owner must be consumed before its frame returns");
    result
}

#[cfg(not(test))]
unsafe extern "C" {
    /// Links a complete stack activation with a pure Rust owner visitor.
    fn __elephc_eval_owner_frame_enter_v1(
        record: *mut c_void, visitor: unsafe extern "C" fn(*const c_void, *mut c_void),
        data: *const c_void,
    );
    /// Splices out this exact activation without executing PHP or releasing values.
    fn __elephc_eval_owner_frame_leave_v1(record: *mut c_void);
    /// Forwards the real owning field to the collector's current slot-identity context.
    fn __elephc_eval_owner_frame_slot_v1(slot: *const *mut RuntimeCell, context: *mut c_void);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scope visitor follows current ownership after insertion, replacement and unset.
    #[test]
    fn runtime_scope_owners_follow_live_slots_without_snapshot_leases() {
        use crate::scope::{ElephcEvalScope, ScopeCellOwnership};
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let mut scope = ElephcEvalScope::new();
        with_runtime_scope_owners(&mut scope, |scope| {
            scope.set("owned", first, ScopeCellOwnership::Owned);
            scope.set("alias", first, ScopeCellOwnership::Borrowed);
            let owners = RuntimeScopeOwners(std::ptr::from_mut(scope));
            let mut seen = Vec::new();
            owners.visit_owned_slots(&mut |slot| seen.push((slot as usize, unsafe { *slot })));
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].1, first.as_ptr());
            assert_eq!(scope.set("owned", second, ScopeCellOwnership::Owned), Some(first));
            seen.clear();
            owners.visit_owned_slots(&mut |slot| seen.push((slot as usize, unsafe { *slot })));
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].1, second.as_ptr());
            assert_eq!(scope.unset("owned"), Some(second));
            owners.visit_owned_slots(&mut |_| panic!("an unset scope entry owns no lease"));
        });
    }

    /// A value remains one real slot until take detaches it before any possible reentry.
    #[test]
    fn runtime_owner_slots_visit_actual_fields_and_clear_before_cleanup() {
        let cell = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        with_owned_runtime_cell(cell, |outer| {
            assert!(outer.borrowed().is_borrowed());
            let mut addresses = Vec::new();
            outer.visit_owned_slots(&mut |slot| { addresses.push(slot as usize); assert_eq!(unsafe { *slot }, cell.as_ptr()); });
            with_owned_runtime_cell(cell, |inner| {
                inner.visit_owned_slots(&mut |slot| { assert_ne!(slot as usize, addresses[0]); });
                let taken = inner.take();
                assert_eq!(taken, cell);
                assert!(!taken.is_borrowed(), "taking transfers the owning lease");
                inner.visit_owned_slots(&mut |_| panic!("a detached owner cannot become a saved-frame edge"));
            });
            outer.visit_owned_slots(&mut |slot| assert_eq!(slot as usize, addresses[0]));
            assert_eq!(outer.take(), cell);
            outer.visit_owned_slots(&mut |_| panic!("cleanup must observe an empty owner slot"));
        });
    }
}
