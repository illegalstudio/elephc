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
use std::cell::Cell;
#[cfg(not(test))]
use std::{ffi::c_void, marker::{PhantomData, PhantomPinned}, pin::Pin};

/// One actual Rust owner whose address remains stable across the native call and cleanup.
pub(crate) struct RuntimeOwnerSlot(Cell<RuntimeCellHandle>);

/// Borrows actual owning fields while a frozen activation cannot mutate its inventory.
pub(crate) trait RuntimeOwners {
    /// Reports each independent lease without acquiring a reference or executing PHP.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell));
}

impl RuntimeOwners for RuntimeCellHandle {
    /// Borrowed metadata contributes no owner beyond its original storage slot.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if !self.is_borrowed() && !self.is_null() { visit(self.owner_slot()); }
    }
}

impl<T: RuntimeOwners> RuntimeOwners for [T] {
    /// Walks the actual stable element fields rather than copying pointer values.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        for owner in self { owner.visit_owned_slots(visit); }
    }
}

impl<T: RuntimeOwners> RuntimeOwners for Vec<T> {
    /// Borrows vector storage, whose shape remains immutable through the guarded operation.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.as_slice().visit_owned_slots(visit);
    }
}

impl<T: RuntimeOwners + ?Sized> RuntimeOwners for &T {
    /// Forwards a borrowed inventory while preserving every original slot identity.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        (**self).visit_owned_slots(visit);
    }
}

impl<A: RuntimeOwners, B: RuntimeOwners> RuntimeOwners for (A, B) {
    /// Combines separate owner inventories without introducing duplicate leases.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.0.visit_owned_slots(visit);
        self.1.visit_owned_slots(visit);
    }
}

impl<T: RuntimeOwners> RuntimeOwners for Option<T> {
    /// An absent result or failed operation contributes no owning field.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if let Some(owner) = self { owner.visit_owned_slots(visit); }
    }
}

impl RuntimeOwnerSlot {
    /// Moves one cleanup obligation, preserving borrowed provenance when no lease exists.
    pub(crate) fn new(value: RuntimeCellHandle) -> Self { Self(Cell::new(value)) }

    /// Lends the cell to a native consumer without transferring its owning lease.
    pub(crate) fn borrowed(&self) -> RuntimeCellHandle { self.0.get().borrowed() }

    /// Detaches the owner before release can execute PHP or suspend again.
    pub(crate) fn take(&self) -> RuntimeCellHandle {
        self.0.replace(RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())
    }

    /// Reads an immobile owning pointer field during a pure, nonmutating collector callback.
    fn visit_owned_slot(&self, mut visit: impl FnMut(*const *mut RuntimeCell)) {
        // The native visitor cannot invoke PHP or mutate this Cell while the shared view exists.
        let value = unsafe { &*self.0.as_ptr() };
        value.visit_owned_slots(&mut visit);
    }
}

impl RuntimeOwners for RuntimeOwnerSlot {
    /// Resolves the currently owned field after any earlier cleanup cleared it.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.visit_owned_slot(visit);
    }
}

/// Keeps an immobile Rust inventory visible through native calls and explicit slot cleanup.
pub(crate) fn with_runtime_owners<T: RuntimeOwners, R>(owners: &T, consume: impl FnOnce() -> R) -> R {
    #[cfg(not(test))]
    let mut record = std::pin::pin!(NativeOwnerFrame { words: [0; 6], _pin: PhantomPinned });
    #[cfg(not(test))]
    let _guard = NativeOwnerFrameGuard::enter(record.as_mut(), owners);
    #[cfg(test)]
    let _ = owners;
    consume()
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
    debug_assert!(owner.0.get().is_null(), "the runtime owner must be consumed before its frame returns");
    result
}

/// Extended native activation prefix, pinned until its Rust owner has finished cleanup.
#[cfg(not(test))]
#[repr(C, align(16))]
struct NativeOwnerFrame { words: [u64; 6], _pin: PhantomPinned }

/// Detaches only its own activation after all native boundaries have returned through Rust.
#[cfg(not(test))]
struct NativeOwnerFrameGuard<'a, T: RuntimeOwners> {
    record: *mut c_void,
    _borrow: PhantomData<(&'a mut NativeOwnerFrame, &'a T)>,
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

#[cfg(not(test))]
impl<'a, T: RuntimeOwners> NativeOwnerFrameGuard<'a, T> {
    /// Exposes a pinned record and its immobile owner only for this lexical borrow.
    fn enter(record: Pin<&'a mut NativeOwnerFrame>, owners: &'a T) -> Self {
        // The guard's borrow prevents moving the record until the native link is removed.
        let record = unsafe { record.get_unchecked_mut() }.words.as_mut_ptr().cast();
        unsafe { __elephc_eval_owner_frame_enter_v1(record, visit::<T>, (owners as *const T).cast()); }
        Self { record, _borrow: PhantomData }
    }
}

#[cfg(not(test))]
impl<T: RuntimeOwners> Drop for NativeOwnerFrameGuard<'_, T> {
    /// Removes the borrowed record before its stack storage or Rust owner can disappear.
    fn drop(&mut self) { unsafe { __elephc_eval_owner_frame_leave_v1(self.record); } }
}

/// Borrows the owning pointer field without retaining PHP values or invoking PHP.
///
/// # Safety
/// Data points to the guard's live immobile inventory. The native context and callback
/// remain pure for this visit, and no pointer-field address survives the collector traversal.
#[cfg(not(test))]
unsafe extern "C" fn visit<T: RuntimeOwners>(data: *const c_void, context: *mut c_void) {
    let owners = unsafe { &*data.cast::<T>() };
    owners.visit_owned_slots(&mut |slot| unsafe { __elephc_eval_owner_frame_slot_v1(slot, context); });
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
            outer.visit_owned_slot(|slot| { addresses.push(slot as usize); assert_eq!(unsafe { *slot }, cell.as_ptr()); });
            with_owned_runtime_cell(cell, |inner| {
                inner.visit_owned_slot(|slot| { assert_ne!(slot as usize, addresses[0]); });
                let taken = inner.take();
                assert_eq!(taken, cell);
                assert!(!taken.is_borrowed(), "taking transfers the owning lease");
                inner.visit_owned_slot(|_| panic!("a detached owner cannot become a saved-frame edge"));
            });
            outer.visit_owned_slot(|slot| assert_eq!(slot as usize, addresses[0]));
            assert_eq!(outer.take(), cell);
            outer.visit_owned_slot(|_| panic!("cleanup must observe an empty owner slot"));
        });
    }
}
