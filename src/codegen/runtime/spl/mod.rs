//! Purpose:
//! Collects runtime helpers for SPL classes whose PHP surface is backed by custom storage.
//! The current module owns the Phase 4 doubly-linked-list family payload layout.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - SPL payload offsets are shared with object cleanup and allocation helpers.

mod doubly_linked_list;
mod fixed_array;

/// Byte offset from the SPL doubly-linked-list object header to its internal
/// `Mixed`-pointer storage array field. Used by all DLL mutators and iterators.
pub(crate) const SPL_DLL_STORAGE_OFFSET: usize = 8;

/// Byte offset from the SPL doubly-linked-list object header to its iterator
/// index field (i64). Zero-initialized; incremented by `__rt_spl_dll_next_prev`.
pub(crate) const SPL_DLL_ITER_INDEX_OFFSET: usize = 16;

/// Byte offset from the SPL doubly-linked-list object header to its iterator
/// mode bits field (i64). Encodes LIFO/FIFO and DELETE flags; set by
/// `setIteratorMode` and read by insert/iteration helpers.
pub(crate) const SPL_DLL_ITER_MODE_OFFSET: usize = 24;

/// Byte offset from the SplFixedArray object header to its fixed-size
/// `Mixed`-pointer storage array field.
pub(crate) const SPL_FIXED_STORAGE_OFFSET: usize = 8;

/// Emits all runtime helpers for `SplDoublyLinkedList`, `SplStack`, and `SplQueue`
/// for the target architecture. Routes to either ARM64 or x86_64 emitters.
pub(crate) use doubly_linked_list::emit_doubly_linked_list_runtime;
/// Emits all runtime helpers for `SplFixedArray` for the target architecture.
/// Routes to either ARM64 or x86_64 emitters.
pub(crate) use fixed_array::emit_fixed_array_runtime;
