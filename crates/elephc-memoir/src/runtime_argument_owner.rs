//! Purpose:
//! Tracks one argument lease as it moves into a native reference pin.
//!
//! Called from:
//! - Foreign invocation boundaries that lend owners to native parameter binders.
//!
//! Key details:
//! - Shared handles carry provenance for a single owner.
//! - A native bind consumes the actual pin field, which the collector can visit.

use crate::owner_layout::ArgumentPin;
use crate::runtime_owners::{RuntimeCell, RuntimeCellHandle, RuntimeOwners};
use std::{cell::{RefCell, UnsafeCell}, ffi::c_void, rc::Rc};

/// Shared provenance for one independently acquired argument lease.
#[derive(Clone)]
pub struct RuntimeArgumentOwner(Rc<RefCell<Option<ArgumentLease>>>);

/// Stores the original owner or its stable replacement pin.
struct ArgumentLease {
    value: RuntimeCellHandle,
    pin: Option<Box<UnsafeCell<ArgumentPin>>>,
}

impl RuntimeArgumentOwner {
    /// Adopts an already acquired owner without retaining it again.
    pub fn new(value: RuntimeCellHandle) -> Self {
        Self(Rc::new(RefCell::new(Some(ArgumentLease { value, pin: None }))))
    }

    /// Reads the original lease identity without keeping a borrow across native operations.
    pub fn value(&self) -> Option<RuntimeCellHandle> {
        self.0.borrow().as_ref().map(|lease| lease.value)
    }

    /// Publishes a replacement pin and returns the previous owner for release.
    /// Installation may write and link the pin, but may not reenter argument ownership or PHP.
    pub fn try_pin<E>(
        &self,
        cell: u64,
        staging: *mut c_void,
        install: impl FnOnce(RuntimeCellHandle, *mut c_void, u64, *mut c_void) -> Result<bool, E>,
    ) -> Result<Option<RuntimeCellHandle>, E> {
        let value = {
            let lease = self.0.borrow();
            let Some(lease) = lease.as_ref() else { return Ok(None); };
            if lease.pin.is_some() || lease.value.is_borrowed() { return Ok(None); }
            lease.value
        };
        let pin = Box::new(UnsafeCell::new(ArgumentPin::default()));
        if !install(value, pin.get().cast(), cell, staging)? { return Ok(None); }
        self.0.borrow_mut().as_mut().expect("lease remains live during pin installation").pin = Some(pin);
        Ok(Some(value))
    }

    /// Detaches the last owner, unlinking a published pin before freeing its storage.
    /// The callback must unlink the pin on every outcome and return its owner if still pending.
    pub fn take<E>(
        &self,
        unlink: impl FnOnce(*mut c_void) -> Result<Option<RuntimeCellHandle>, E>,
    ) -> Result<Option<RuntimeCellHandle>, E> {
        let lease = self.0.borrow_mut().take();
        let Some(lease) = lease else { return Ok(None); };
        match lease.pin {
            Some(pin) => unlink(pin.get().cast()),
            None => Ok(Some(lease.value)),
        }
    }
}

impl RuntimeOwners for RuntimeArgumentOwner {
    /// Visits the native pin's actual owner field when a bind has yet to consume it.
    #[allow(unsafe_code)]
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        let lease = self.0.borrow();
        if let Some(lease) = lease.as_ref() {
            if let Some(pin) = &lease.pin {
                let slot = unsafe { &raw const (*pin.get()).validation.owner }.cast::<*mut RuntimeCell>();
                if !unsafe { *slot }.is_null() { visit(slot); }
            } else {
                lease.value.visit_owned_slots(visit);
            }
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

    /// Pending native pins expose their actual owner field, and native consumption removes it.
    #[test]
    fn pending_argument_pin_owner_slot_disappears_at_native_consumption() {
        let value = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let owner = RuntimeArgumentOwner::new(value);
        let mut record = std::ptr::null_mut::<ArgumentPin>();
        let old = owner.try_pin(16, std::ptr::null_mut(), |value, storage, _, _| {
            record = storage.cast();
            unsafe {
                (*record).validation.owner = value.as_ptr() as u64;
                (*record).validation.kind = ArgumentPin::PENDING as u64;
            }
            Ok::<_, ()>(true)
        });
        assert_eq!(old, Ok(Some(value)));
        let mut slots = Vec::new();
        owner.visit_owned_slots(&mut |slot| {
            assert_eq!(unsafe { *slot }, value.as_ptr());
            slots.push(slot as usize);
        });
        assert_eq!(slots, [unsafe { record.cast::<u8>().add(ArgumentPin::OWNER) } as usize],
            "only the native pin field owns the transferred source lease");
        // This is the native activate transition, which clears before releasing the source.
        unsafe {
            (*record).validation.owner = 0;
            (*record).validation.kind = ArgumentPin::ACTIVE as u64;
        }
        owner.visit_owned_slots(&mut |_| panic!("the consumed pin and stale descriptor own no value"));
        assert_eq!(owner.take(|_| Ok::<_, ()>(None)), Ok(None));
        assert_eq!(owner.take(|_| panic!("the pin was already detached")), Ok::<_, ()>(None));
    }
}
