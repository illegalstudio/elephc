//! Purpose:
//! Names the opaque runtime cell/value handles used by eval internals.
//! Prevents the eval bridge from introducing a second PHP value system.
//!
//! Called from:
//! - `crate::scope`, `crate::interpreter`, and the eval FFI adapters.
//!
//! Key details:
//! - Handles carry Rust-only result provenance; native ABI slots contain raw pointers only.
//! - Copying a handle does not retain its cell. Borrowed reads must acquire a lease before cleanup.

use std::ffi::c_void;

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
        Self {
            ptr,
            borrowed: false,
        }
    }

    /// Returns the raw runtime-cell pointer for ABI calls back into elephc.
    pub const fn as_ptr(self) -> *mut RuntimeCell {
        self.ptr
    }

    /// Returns true when this handle does not reference a runtime cell.
    pub const fn is_null(self) -> bool {
        self.ptr.is_null()
    }

    /// Marks a storage read as borrowed without changing the runtime reference count.
    pub(crate) const fn borrowed(self) -> Self {
        Self {
            borrowed: true,
            ..self
        }
    }

    /// Records a retained or transferred owner without changing the runtime reference count.
    #[cfg(test)]
    pub(crate) const fn owned(self) -> Self {
        Self {
            borrowed: false,
            ..self
        }
    }

    /// Returns whether the expression must retain this cell before consuming it.
    pub(crate) const fn is_borrowed(self) -> bool {
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
