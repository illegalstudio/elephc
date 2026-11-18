//! Purpose:
//! Exposes the native storage emitter to the compiler runtime.
//!
//! Called from:
//! - The compiler's managed runtime emitter inventory.
//!
//! Key details:
//! - Assembly and symbol selection enter through the established compiler adapter.

use crate::codegen_support::emit::Emitter;

/// Emits the owned bucket insertion with the compiler's automatic integer key policy.
pub fn emit_hash_insert_owned(emitter: &mut Emitter) {
    elephc_memoir::values::hash_insert_owned::emit_hash_insert_owned(
        emitter, super::hash_next_index::record_insert,
    );
}
