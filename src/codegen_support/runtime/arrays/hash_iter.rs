//! Purpose:
//! Exposes the native storage emitter to the compiler runtime.
//!
//! Called from:
//! - The compiler's managed runtime emitter inventory.
//!
//! Key details:
//! - Assembly and symbol selection enter through the established compiler adapter.

use crate::codegen_support::emit::Emitter;

/// Emits the raw bucket cursor followed by the PHP value and iterator-resume adapters.
pub fn emit_hash_iter(emitter: &mut Emitter) {
    elephc_memoir::values::hash_iter::emit_raw_hash_iter(emitter);
    super::hash_entry_reference::emit_hash_entry_reference(emitter);
}
