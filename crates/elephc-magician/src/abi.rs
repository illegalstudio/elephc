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

pub use elephc_memoir::owner_layout::EvalResultWords as ElephcEvalResult;

pub use crate::context::ElephcEvalContext;
pub use crate::scope::ElephcEvalScope;

/// ABI version shared by generated call sites and the eval bridge.
pub const ABI_VERSION: u32 = 2;

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
/// Scope-set input flag marking a synchronized native snapshot, rather than an eval write.
pub const SCOPE_FLAG_NATIVE_SNAPSHOT: u32 = 1 << 5;
/// A native PHP name was detached by unset, including when eval subsequently rebound it.
pub const SCOPE_FLAG_NATIVE_DETACHED: u32 = 1 << 6;
