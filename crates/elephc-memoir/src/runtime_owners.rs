//! Purpose:
//! Owns opaque native cell handles, runtime owner slots and protected retirement.
//!
//! Called from:
//! - Magician's scope, interpreter, and the eval FFI adapters.
//!
//! Key details:
//! - Handles carry Rust-only result provenance; native ABI slots contain raw pointers only.
//! - Copying a handle does not retain its cell. Borrowed reads must acquire a lease before cleanup.

use std::{cell::Cell, ffi::c_void};

/// Opaque pointer to an elephc runtime cell.
pub type RuntimeCell = c_void;

/// Wraps a runtime pointer and records whether an expression borrows an existing owner's cell.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeCellHandle {
    ptr: *mut RuntimeCell,
    borrowed: bool,
}

impl RuntimeCellHandle {
    /// Accepts ownership transferred through a raw runtime-cell pointer.
    ///
    /// Storage lookup paths must explicitly mark their returned view as borrowed.
    pub const fn from_raw(ptr: *mut RuntimeCell) -> Self {
        Self { ptr, borrowed: false }
    }

    /// Returns the raw runtime-cell pointer for ABI calls back into elephc.
    pub const fn as_ptr(self) -> *mut RuntimeCell {
        self.ptr
    }

    /// Borrows the actual pointer field for a pure collector owner-slot callback.
    /// The enclosing owner must stay immobile and unchanged through the callback.
    pub fn owner_slot(&self) -> *const *mut RuntimeCell {
        &self.ptr
    }

    /// Returns true when this handle does not reference a runtime cell.
    pub const fn is_null(self) -> bool {
        self.ptr.is_null()
    }

    /// Marks a storage read as borrowed without changing the runtime reference count.
    pub const fn borrowed(self) -> Self {
        Self { borrowed: true, ..self }
    }

    /// Returns whether the expression must retain this cell before consuming it.
    pub const fn is_borrowed(self) -> bool {
        self.borrowed
    }
}

impl PartialEq for RuntimeCellHandle {
    /// Compares cell identity independently of how the current expression obtained its handle.
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl Eq for RuntimeCellHandle {}

/// One actual Rust owner whose address remains stable across the native call and cleanup.
pub struct RuntimeOwnerSlot(Cell<RuntimeCellHandle>);

/// Borrows actual owning fields while a frozen activation cannot mutate its inventory.
pub trait RuntimeOwners {
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
    pub fn new(value: RuntimeCellHandle) -> Self { Self(Cell::new(value)) }

    /// Lends the cell to a native consumer without transferring its owning lease.
    pub fn borrowed(&self) -> RuntimeCellHandle { self.0.get().borrowed() }

    /// Detaches the owner before release can execute PHP or suspend again.
    pub fn take(&self) -> RuntimeCellHandle {
        self.0.replace(RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())
    }

    /// Reports whether this slot has already relinquished its lease.
    pub fn is_empty(&self) -> bool {
        self.0.get().is_null()
    }
}

impl RuntimeOwners for RuntimeOwnerSlot {
    /// Resolves the actual pointer field during a pure collector visit.
    #[allow(unsafe_code)]
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        let cell = self.0.get();
        if !cell.is_borrowed() && !cell.is_null() {
            // A raw field address does not borrow the Cell's contents. The consumer may
            // only read it while this slot stays immobile and unchanged.
            visit(unsafe { &raw const (*self.0.as_ptr()).ptr });
        }
    }
}

/// Consumes every lease through a contained native callback, accumulating its owned Throwable.
///
/// The callback returns whether this release failed. Previously accumulated exceptions can
/// survive successful releases without setting the failure bit.
pub fn release_owned_cells(
    cells: impl IntoIterator<Item = RuntimeCellHandle>,
    pending: Option<RuntimeCellHandle>,
    mut release: impl FnMut(*mut RuntimeCell, &mut *mut RuntimeCell) -> bool,
) -> (Option<RuntimeCellHandle>, bool) {
    let mut thrown = pending.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr);
    let mut caught = false;
    for cell in cells {
        caught |= release(cell.as_ptr(), &mut thrown);
    }
    ((!thrown.is_null()).then(|| RuntimeCellHandle::from_raw(thrown)), caught)
}
