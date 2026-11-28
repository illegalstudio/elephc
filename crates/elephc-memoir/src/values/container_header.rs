//! Purpose:
//! Centralizes the logical-count word shared by indexed arrays and associative hashes.
//!
//! Called from:
//! - Runtime helpers and EIR lowering that inspect either native PHP container layout.
//!
//! Key details:
//! - Both layouts expose their live logical element count in the first payload word.
//! - Callers select semantics, while Memoir owns the physical offset and load/store form.

use crate::Emitter;

/// Byte offset of the logical element count in every native PHP container payload.
pub const COUNT_OFFSET: usize = 0;

/// Loads the logical element count without exposing the shared header offset.
pub fn emit_load_count(emitter: &mut impl Emitter, destination: &str, container: &str) {
    emitter.emit_load_from_address(destination, container, COUNT_OFFSET);
}

/// Stores the logical element count without exposing the shared header offset.
pub fn emit_store_count(emitter: &mut impl Emitter, source: &str, container: &str) {
    emitter.emit_store_to_address(source, container, COUNT_OFFSET);
}
