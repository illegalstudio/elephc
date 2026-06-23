//! Purpose:
//! Names the opaque runtime cell/value handles used by eval internals.
//! Prevents the eval bridge from introducing a second PHP value system.
//!
//! Called from:
//! - Future `crate::scope` and `crate::interpreter` implementations.
//!
//! Key details:
//! - Handles point at elephc runtime cells whose tag/payload/refcount contract
//!   is owned by the main runtime.

use std::ffi::c_void;

/// Opaque pointer to an elephc runtime cell.
pub type RuntimeCell = c_void;

/// Wraps an opaque runtime cell pointer without taking ownership by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeCellHandle {
    ptr: *mut RuntimeCell,
}

impl RuntimeCellHandle {
    /// Creates a runtime-cell handle from a raw pointer supplied by elephc.
    pub const fn from_raw(ptr: *mut RuntimeCell) -> Self {
        Self { ptr }
    }

    /// Returns the raw runtime-cell pointer for ABI calls back into elephc.
    pub const fn as_ptr(self) -> *mut RuntimeCell {
        self.ptr
    }

    /// Returns true when this handle does not reference a runtime cell.
    pub const fn is_null(self) -> bool {
        self.ptr.is_null()
    }
}
