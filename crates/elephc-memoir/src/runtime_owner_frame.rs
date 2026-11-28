//! Purpose:
//! Links a stack-pinned inventory of temporary native cell owners to an activation chain.
//!
//! Called from:
//! - Foreign runtimes with suspended-stack collector callbacks.
//!
//! Key details:
//! - The visitor lends actual pointer-field addresses for the duration of traversal.
//! - The guard unlinks before either its pinned storage or its owner inventory expires.

use crate::owner_layout::ExtendedActivation;
use crate::runtime_owners::{RuntimeCell, RuntimeOwners};
use std::{cell::UnsafeCell, ffi::c_void, marker::{PhantomData, PhantomPinned}, pin::Pin};

/// Native operations for publishing, visiting and detaching an owner inventory.
pub struct OwnerFrameHooks {
    /// Links the activation with its pure visitor and inventory address.
    pub enter: unsafe extern "C" fn(*mut c_void, unsafe extern "C" fn(*const c_void, *mut c_void), *const c_void),
    /// Unlinks the given activation without executing guest code.
    pub leave: unsafe extern "C" fn(*mut c_void),
    /// Records one owning field in the current collector traversal.
    pub visit_slot: unsafe extern "C" fn(*const *mut RuntimeCell, *mut c_void),
}

/// Extended activation storage, immobile while the native chain links to it.
struct NativeOwnerFrame {
    storage: UnsafeCell<ExtendedActivation>,
    _pin: PhantomPinned,
}

/// Binds the owning inventory and visitor for the life of the activation.
struct Inventory<'a, T> {
    owners: &'a T,
    visit_slot: unsafe extern "C" fn(*const *mut RuntimeCell, *mut c_void),
}

/// Mutable inventory addressed only while its owning thread is suspended in a runtime callback.
struct MutableInventory<T> {
    owners: *mut T,
    visit_slot: unsafe extern "C" fn(*const *mut RuntimeCell, *mut c_void),
}

/// Unlinks the activation before the pinned stack words can go out of scope.
struct NativeOwnerFrameGuard<'a> {
    record: *mut c_void,
    leave: unsafe extern "C" fn(*mut c_void),
    _borrow: PhantomData<&'a mut NativeOwnerFrame>,
}

impl Drop for NativeOwnerFrameGuard<'_> {
    /// Splices out this record, including when the Rust closure panics.
    fn drop(&mut self) { unsafe { (self.leave)(self.record); } }
}

/// Publishes a live Rust inventory during the scoped operation.
///
/// # Safety
/// Hooks must use the shared activation layout, unlink the exact record in `leave`, and
/// traverse only while the inventory is live and unchanged. They cannot unwind through FFI
/// or keep any borrowed data or slot addresses after traversal.
pub unsafe fn with_owner_frame<T: RuntimeOwners, R>(
    owners: &T,
    hooks: OwnerFrameHooks,
    consume: impl FnOnce() -> R,
) -> R {
    let mut frame = std::pin::pin!(NativeOwnerFrame { storage: UnsafeCell::default(), _pin: PhantomPinned });
    let inventory = Inventory { owners, visit_slot: hooks.visit_slot };
    let _guard = NativeOwnerFrameGuard::link(frame.as_mut(), &inventory, hooks);
    consume()
}

/// Publishes an inventory whose contents may change between runtime suspension points.
///
/// # Safety
/// Hooks must satisfy `with_owner_frame` and invoke the visitor only while `consume` is
/// suspended, never concurrently with a mutation. The inventory object itself must remain at
/// the same address until `leave` returns. Traversal cannot retain references or slot addresses.
pub unsafe fn with_mutable_owner_frame<T: RuntimeOwners, R>(
    owners: &mut T,
    hooks: OwnerFrameHooks,
    consume: impl FnOnce(&mut T) -> R,
) -> R {
    let frame = std::pin::pin!(NativeOwnerFrame { storage: UnsafeCell::default(), _pin: PhantomPinned });
    let inventory = MutableInventory {
        owners: std::ptr::from_mut(owners),
        visit_slot: hooks.visit_slot,
    };
    let record = frame.as_ref().get_ref().storage.get().cast();
    unsafe {
        (hooks.enter)(
            record,
            visit_mutable::<T>,
            std::ptr::from_ref(&inventory).cast(),
        );
    }
    let _guard = NativeOwnerFrameGuard {
        record,
        leave: hooks.leave,
        _borrow: PhantomData,
    };
    consume(owners)
}

impl<'a> NativeOwnerFrameGuard<'a> {
    /// Ties the lifetime of the frame and inventory to the native link.
    fn link<T: RuntimeOwners>(
        frame: Pin<&'a mut NativeOwnerFrame>,
        inventory: &'a Inventory<'_, T>, hooks: OwnerFrameHooks,
    ) -> Self {
        let record = frame.as_ref().get_ref().storage.get().cast();
        unsafe { (hooks.enter)(record, visit::<T>, std::ptr::from_ref(inventory).cast()); }
        Self { record, leave: hooks.leave, _borrow: PhantomData }
    }
}

/// Reads the current inventory and lends every owning slot to the active traversal.
///
/// # Safety
/// The caller must pass the live inventory and a context accepted by its pure slot callback.
unsafe extern "C" fn visit<T: RuntimeOwners>(data: *const c_void, context: *mut c_void) {
    let inventory = unsafe { &*data.cast::<Inventory<'_, T>>() };
    inventory.owners.visit_owned_slots(&mut |slot| unsafe { (inventory.visit_slot)(slot, context); });
}

/// Reads the inventory's current shape after the mutating thread has suspended.
///
/// # Safety
/// The caller must pass the live mutable inventory while no mutation is in progress.
unsafe extern "C" fn visit_mutable<T: RuntimeOwners>(data: *const c_void, context: *mut c_void) {
    let inventory = unsafe { &*data.cast::<MutableInventory<T>>() };
    let owners = unsafe { &*inventory.owners };
    owners.visit_owned_slots(&mut |slot| unsafe { (inventory.visit_slot)(slot, context); });
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::runtime_owners::RuntimeCellHandle;
    use std::cell::Cell;

    thread_local! {
        static ACTIVE_VISITOR: Cell<Option<unsafe extern "C" fn(*const c_void, *mut c_void)>> = const { Cell::new(None) };
        static ACTIVE_DATA: Cell<*const c_void> = const { Cell::new(std::ptr::null()) };
    }

    /// Captures the frame visitor so the test can simulate a suspended collector traversal.
    unsafe extern "C" fn enter_test_frame(
        _record: *mut c_void,
        visitor: unsafe extern "C" fn(*const c_void, *mut c_void),
        data: *const c_void,
    ) {
        ACTIVE_VISITOR.set(Some(visitor));
        ACTIVE_DATA.set(data);
    }

    /// Clears the simulated native activation link.
    unsafe extern "C" fn leave_test_frame(_record: *mut c_void) {
        ACTIVE_VISITOR.set(None);
        ACTIVE_DATA.set(std::ptr::null());
    }

    /// Appends the currently visible owner value to the traversal result.
    unsafe extern "C" fn record_test_slot(slot: *const *mut RuntimeCell, context: *mut c_void) {
        let seen = unsafe { &mut *context.cast::<Vec<usize>>() };
        seen.push(unsafe { *slot } as usize);
    }

    /// Runs the linked visitor exactly as the native collector would at a suspension point.
    fn collect_test_slots() -> Vec<usize> {
        let mut seen = Vec::new();
        let visitor = ACTIVE_VISITOR.get().expect("the owner frame is linked");
        let data = ACTIVE_DATA.get();
        unsafe { visitor(data, std::ptr::from_mut(&mut seen).cast()); }
        seen
    }

    /// A mutable owner frame exposes additions and removals made between traversals.
    #[test]
    fn mutable_owner_frame_visits_the_current_inventory_shape() {
        let first = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let second = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let mut owners = vec![first];
        let hooks = OwnerFrameHooks {
            enter: enter_test_frame,
            leave: leave_test_frame,
            visit_slot: record_test_slot,
        };

        unsafe {
            with_mutable_owner_frame(&mut owners, hooks, |owners| {
                assert_eq!(collect_test_slots(), [8]);
                owners.push(second);
                assert_eq!(collect_test_slots(), [8, 16]);
                owners.remove(0);
                assert_eq!(collect_test_slots(), [16]);
            });
        }
        assert!(ACTIVE_VISITOR.get().is_none());
    }
}
