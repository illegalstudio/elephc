//! Purpose:
//! Holds stable, foreign-writable staging for a native by-reference argument.
//!
//! Called from:
//! - Magician's descriptor invoker adapters.
//!
//! Key details:
//! - The staged pair has the same offsets as the generated native writer.
//! - The caller owns and retires any PHP payload stored in the pair.

use core::{cell::{Cell, UnsafeCell}, ffi::c_void, marker::PhantomPinned, pin::Pin};

use crate::owner_layout::{InvokerSlotWords, ValidationPin};
use crate::runtime_owners::{RuntimeCell, RuntimeCellHandle, RuntimeOwners};

/// Address-stable storage that native code can write through a reference marker.
pub struct InvokerSlot {
    words: UnsafeCell<InvokerSlotWords>,
    _pin: PhantomPinned,
}

impl InvokerSlot {
    /// Initializes both words before the caller publishes a native marker.
    pub fn pin(words: InvokerSlotWords) -> Pin<Box<Self>> {
        Box::pin(Self { words: UnsafeCell::new(words), _pin: PhantomPinned })
    }

    /// Lends the stable address to a native marker for this slot's lifetime.
    pub fn as_ptr(self: Pin<&Self>) -> *mut InvokerSlotWords {
        self.words.get()
    }

    /// Copies the current words after native control returns to the owner.
    #[allow(unsafe_code)]
    pub fn snapshot(self: Pin<&Self>) -> InvokerSlotWords {
        // This slot is !Sync, and copying these words cannot reenter its native writer.
        unsafe { self.as_ptr().read() }
    }

    /// Clears the foreign-visible pair before transferring its payload to cleanup.
    #[allow(unsafe_code)]
    pub fn take(self: Pin<&Self>) -> InvokerSlotWords {
        // Control is back in Rust. A later callback can only observe the cleared pair.
        unsafe { self.as_ptr().replace(InvokerSlotWords::new(0, 0)) }
    }
}

/// Address-stable, foreign-writable storage for an owned Mixed-cell reference slot.
pub struct InvokerMixedSlot(Box<UnsafeCell<*mut RuntimeCell>>);

impl InvokerMixedSlot {
    /// Adopts the initial cell lease without retaining it again.
    pub fn new(cell: RuntimeCellHandle) -> Self {
        debug_assert!(!cell.is_borrowed());
        Self(Box::new(UnsafeCell::new(cell.as_ptr())))
    }

    /// Lends the address of the actual owner field to the native writer.
    pub fn as_ptr(&self) -> *mut *mut RuntimeCell {
        self.0.get()
    }

    /// Gives cleanup the current cell after clearing the actual owner field.
    #[allow(unsafe_code)]
    pub fn take(&self) -> Option<RuntimeCellHandle> {
        let cell = unsafe { self.as_ptr().replace(core::ptr::null_mut()) };
        (!cell.is_null()).then(|| RuntimeCellHandle::from_raw(cell))
    }
}

impl RuntimeOwners for InvokerMixedSlot {
    /// Visits the native writer's owner field while it holds a cell.
    #[allow(unsafe_code)]
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        let slot = self.as_ptr();
        if !unsafe { *slot }.is_null() {
            visit(slot);
        }
    }
}

/// Keeps a managed reference validation record stable until native unlink completes.
#[derive(Default)]
pub struct InvokerValidationPin {
    record: Box<UnsafeCell<ValidationPin>>,
    retired: Cell<bool>,
}

impl InvokerValidationPin {
    /// Lends storage to the native validation chain and its pending-argument pins.
    pub fn as_ptr(&self) -> *mut c_void {
        self.record.get().cast()
    }

    /// Retires the record once while keeping its address valid throughout native cleanup.
    /// The supplied callback must unlink the native record before releasing its owner.
    pub fn retire<E>(
        &self,
        unlink: impl FnOnce(*mut c_void) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.retired.replace(true) {
            return Ok(());
        }
        unlink(self.as_ptr())
    }
}

impl InvokerSlotWords {
    /// Copies a native slot whose representation contains two initialized words.
    ///
    /// # Safety
    /// `address` must point to a live, aligned two-word slot that can be read now.
    #[allow(unsafe_code)]
    pub unsafe fn read_at(address: usize) -> Self {
        unsafe { (address as *const Self).read() }
    }

    /// Stores a complete scalar pair into the caller's native slot.
    ///
    /// # Safety
    /// `address` must point to a live, aligned two-word slot writable by the caller.
    #[allow(unsafe_code)]
    pub unsafe fn write_at(self, address: usize) {
        unsafe { (address as *mut Self).write(self); }
    }

    /// Installs a complete pair and returns the previous pair's ownership to the caller.
    ///
    /// # Safety
    /// `address` must point to a live, initialized two-word slot writable by the caller.
    #[allow(unsafe_code)]
    pub unsafe fn replace_at(self, address: usize) -> Self {
        unsafe { (address as *mut Self).replace(self) }
    }
}


#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

    /// The collector follows the actual cell field, and retirement clears it exactly once.
    #[test]
    fn invoker_mixed_slot_tracks_native_replacement_until_take() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let slot = InvokerMixedSlot::new(first);
        let address = slot.as_ptr();
        unsafe { address.write(second.as_ptr()); }
        let mut visits = Vec::new();
        slot.visit_owned_slots(&mut |field| {
            assert_eq!(field, address.cast_const());
            visits.push(unsafe { *field });
        });
        assert_eq!(visits, [second.as_ptr()]);
        assert_eq!(slot.take(), Some(second));
        assert_eq!(slot.take(), None);
        slot.visit_owned_slots(&mut |_| panic!("a retired slot has no collector edge"));
    }

    /// A failed protected release retires the native validation record exactly once.
    #[test]
    fn invoker_validation_pin_keeps_address_through_failed_retirement() {
        let pin = InvokerValidationPin::default();
        let address = pin.as_ptr();
        assert_eq!(pin.retire(|record| {
            assert_eq!(record, address);
            Err("release")
        }), Err("release"));
        assert_eq!(pin.retire(|_| panic!("already retired")), Ok::<(), &str>(()));
    }
}
