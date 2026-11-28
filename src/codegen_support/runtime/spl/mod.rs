//! Purpose:
//! Collects runtime helpers for SPL classes whose PHP surface is backed by custom storage.
//! The current module owns the Phase 4 doubly-linked-list family payload layout.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - SPL payload offsets are shared with object cleanup and allocation helpers.

mod doubly_linked_list;
mod fixed_array;
pub(super) mod offset_convert;

/// Emits all runtime helpers for `SplDoublyLinkedList`, `SplStack`, and `SplQueue`
/// for the target architecture. Routes to either ARM64 or x86_64 emitters.
pub(crate) use doubly_linked_list::emit_doubly_linked_list_runtime;
/// Emits all runtime helpers for `SplFixedArray` for the target architecture.
/// Routes to either ARM64 or x86_64 emitters.
pub(crate) use fixed_array::emit_fixed_array_runtime;
/// Emits the shared SPL offset conversion and offset TypeError helpers.
pub(crate) use offset_convert::emit_spl_offset_runtime;
/// The type-name rows a rejected SPL offset reports, emitted as `_spl_offset_type_rows`.
pub(crate) use offset_convert::{SPL_FLOAT_STRING_PREFIX, SPL_FLOAT_STRING_SUFFIX, SPL_OFFSET_TYPE_ROWS};
