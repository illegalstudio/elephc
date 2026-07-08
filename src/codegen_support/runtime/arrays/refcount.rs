//! Purpose:
//! Emits runtime helper assembly for refcount through `emit_refcount`.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array, hash, heap, GC, and Mixed helpers must preserve runtime layout, refcounts, and COW rules before mutating shared storage.

use crate::codegen_support::emit::Emitter;

use super::decref_array::emit_decref_array;
use super::decref_hash::emit_decref_hash;
use super::decref_object::emit_decref_object;
use super::incref::emit_incref;

/// Reference counting runtime functions for the garbage collector.
/// Refcount is stored as a 32-bit value at [user_ptr - 12] inside the uniform 16-byte heap header.
pub fn emit_refcount(emitter: &mut Emitter) {
    emit_incref(emitter);
    emit_decref_array(emitter);
    emit_decref_hash(emitter);
    emit_decref_object(emitter);
}
