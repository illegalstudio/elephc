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
