//! Purpose:
//! Defines the C-compatible eval bridge ABI structs and version constant.
//! Keeps opaque runtime handles separate from Rust implementation details.
//!
//! Called from:
//! - `crate::__elephc_eval_abi_version()`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - C-visible result structs are `#[repr(C)]`; context/scope cross the ABI as
//!   opaque pointers whose internal Rust layout is not exposed.
//! - Runtime values cross this boundary as opaque cell pointers, not Rust enums.

use std::ffi::c_void;

pub use crate::context::ElephcEvalContext;
pub use crate::scope::ElephcEvalScope;

/// ABI version shared by generated call sites and the eval bridge.
pub const ABI_VERSION: u32 = 1;

/// Scope-entry ABI flag indicating that a variable has a visible value.
pub const SCOPE_FLAG_PRESENT: u32 = 1 << 0;
/// Scope-entry ABI flag indicating that a variable has been unset.
pub const SCOPE_FLAG_UNSET: u32 = 1 << 1;
/// Scope-entry ABI flag indicating that native code must resynchronize this entry.
pub const SCOPE_FLAG_DIRTY: u32 = 1 << 2;
/// Scope-entry ABI flag indicating that the scope entry is by-reference.
pub const SCOPE_FLAG_BY_REF: u32 = 1 << 3;
/// Scope-entry ABI flag indicating that the scope owns the runtime cell handle.
pub const SCOPE_FLAG_OWNED: u32 = 1 << 4;

/// Result storage written by `__elephc_eval_execute`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ElephcEvalResult {
    pub kind: u32,
    pub value_cell: *mut c_void,
    pub error: *mut c_void,
}

impl ElephcEvalResult {
    /// Resets result storage to the normal-null placeholder used by the stub.
    pub fn clear(&mut self) {
        self.kind = 0;
        self.value_cell = std::ptr::null_mut();
        self.error = std::ptr::null_mut();
    }
}
