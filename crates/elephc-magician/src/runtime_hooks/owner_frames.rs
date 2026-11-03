//! Purpose:
//! Publishes Rust-owned temporary cells as edges of a suspended native activation.
//!
//! Called from:
//! - Runtime method and constructor bridges while their argument array remains owned by Rust.
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
pub(super) struct RuntimeOwnerSlot(Cell<RuntimeCellHandle>);

impl RuntimeOwnerSlot {
    /// Lends the cell to a native consumer without transferring its owning lease.
    pub(super) fn borrowed(&self) -> RuntimeCellHandle { self.0.get().borrowed() }

    /// Detaches the owner before release can execute PHP or suspend again.
    pub(super) fn take(&self) -> RuntimeCellHandle {
        self.0.replace(RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())
    }

    /// Reads an immobile owning pointer field during a pure, nonmutating collector callback.
    fn visit_owned_slot(&self, mut visit: impl FnMut(*const *mut RuntimeCell)) {
        // The native visitor cannot invoke PHP or mutate this Cell while the shared view exists.
        let value = unsafe { &*self.0.as_ptr() };
        if !value.is_borrowed() && !value.is_null() { visit(value.owner_slot()); }
    }
}

/// Adopts one temporary owner until the consumer explicitly takes it for cleanup.
pub(super) fn with_owned_runtime_cell<R>(
    value: RuntimeCellHandle, consume: impl FnOnce(&RuntimeOwnerSlot) -> R,
) -> R {
    debug_assert!(!value.is_borrowed());
    let owner = RuntimeOwnerSlot(Cell::new(value));
    #[cfg(not(test))]
    let mut record = std::pin::pin!(NativeOwnerFrame { words: [0; 6], _pin: PhantomPinned });
    #[cfg(not(test))]
    let _guard = NativeOwnerFrameGuard::enter(record.as_mut(), &owner);
    let result = consume(&owner);
    debug_assert!(owner.0.get().is_null(), "the runtime owner must be consumed before its frame returns");
    result
}

/// Extended native activation prefix, pinned until its Rust owner has finished cleanup.
#[cfg(not(test))]
#[repr(C, align(16))]
struct NativeOwnerFrame { words: [u64; 6], _pin: PhantomPinned }

/// Detaches only its own activation after all native boundaries have returned through Rust.
#[cfg(not(test))]
struct NativeOwnerFrameGuard<'a> {
    record: *mut c_void,
    _borrow: PhantomData<(&'a mut NativeOwnerFrame, &'a RuntimeOwnerSlot)>,
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
impl<'a> NativeOwnerFrameGuard<'a> {
    /// Exposes a pinned record and its immobile owner only for this lexical borrow.
    fn enter(record: Pin<&'a mut NativeOwnerFrame>, owner: &'a RuntimeOwnerSlot) -> Self {
        // The guard's borrow prevents moving the record until the native link is removed.
        let record = unsafe { record.get_unchecked_mut() }.words.as_mut_ptr().cast();
        unsafe { __elephc_eval_owner_frame_enter_v1(record, visit, (owner as *const RuntimeOwnerSlot).cast()); }
        Self { record, _borrow: PhantomData }
    }
}

#[cfg(not(test))]
impl Drop for NativeOwnerFrameGuard<'_> {
    /// Removes the borrowed record before its stack storage or Rust owner can disappear.
    fn drop(&mut self) { unsafe { __elephc_eval_owner_frame_leave_v1(self.record); } }
}

/// Borrows the owning pointer field without retaining PHP values or invoking PHP.
///
/// # Safety
/// Data points to the guard's live immobile RuntimeOwnerSlot. The native context and callback
/// remain pure for this visit, and no pointer-field address survives the collector traversal.
#[cfg(not(test))]
unsafe extern "C" fn visit(data: *const c_void, context: *mut c_void) {
    let owner = unsafe { &*data.cast::<RuntimeOwnerSlot>() };
    owner.visit_owned_slot(|slot| unsafe { __elephc_eval_owner_frame_slot_v1(slot, context); });
}

#[cfg(test)]
mod tests {
    use super::*;

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
