//! Purpose:
//! Emits the `__rt_array_unbox_slots` runtime helper: converts an indexed array whose slots
//! are boxed Mixed cells into a fresh indexed array of raw 8-byte payload slots of one kind.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - The by-reference Mixed writeback (`codegen::lower_inst::reference_arguments`), when a
//!   callee replaced a typed `array<T>` argument with an array built from Mixed values.
//!
//! Key details:
//! - Every element must unbox to the expected runtime tag; on the first mismatch the helper
//!   returns 0 and the caller raises the diagnostic (the partial copy is abandoned to the exit).
//! - Refcounted payloads (indexed arrays, hashes, objects, callable descriptors) are retained
//!   by the new array; the source array keeps its own boxed references, so the caller still
//!   owns and releases it.
//! - Scalars and heap pointers fill 8-byte slots; strings (tag 1) fill 16-byte `(ptr, len)`
//!   slots holding an owned copy made by `__rt_str_persist`, the layout `__rt_array_push_str`
//!   produces.

pub use elephc_memoir::values::array_unbox_slots::emit_array_unbox_slots;
