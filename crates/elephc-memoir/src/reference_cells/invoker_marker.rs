//! Purpose:
//! Projects caller storage from descriptor-invoker reference markers.
//!
//! Called from:
//! - The compiler's packed and associative variadic argument writeback.
//!
//! Key details:
//! - The marker borrows a caller slot and carries its native representation.
//! - Callers decide when to retire an old payload and when to free a moved box.

use crate::{Emitter, TargetAbi, layout};

/// Internal Mixed tag identifying an invoker's reference marker.
pub const INVOKER_ARG_REF_CELL_TAG: i64 = 11;

/// Representation of a caller slot storing an inline nullable integer.
pub const INVOKER_ARG_TAGGED_SCALAR_SOURCE: i64 = 12;

/// Borrows the marker's source representation and the caller slot address.
pub fn emit_project(
    emitter: &mut impl TargetAbi, marker: &str, source_shape: &str, address: &str,
) {
    layout::emit_load_mixed_high(emitter, source_shape, marker);
    layout::emit_load_mixed_low(emitter, address, marker);
}

/// Second word of the destination's inline storage.
pub enum HighWord {
    /// Transfer the replacement's second payload word as-is.
    Payload,
    /// Preserve the PHP tag in an inline nullable-scalar slot.
    Tag,
}

/// Moves an already validated box's payload words into the caller's slot.
pub fn emit_publish_box_payload(
    emitter: &mut impl TargetAbi, destination: &str, replacement: &str, scratch: &str, high: HighWord,
) {
    layout::emit_load_mixed_low(emitter, scratch, replacement);
    emitter.emit_store_to_address(scratch, destination, 0);
    match high {
        HighWord::Payload => layout::emit_load_mixed_high(emitter, scratch, replacement),
        HighWord::Tag => layout::emit_load_mixed_tag(emitter, scratch, replacement),
    }
    emitter.emit_store_to_address(scratch, destination, 8);
}

/// Publishes a boxed replacement and returns the displaced owner in a scratch register.
pub fn emit_swap_box(
    emitter: &mut impl TargetAbi, destination: &str, replacement: &str, displaced: &str,
) {
    emitter.emit_load_from_address(displaced, destination, 0);
    emitter.emit_store_to_address(replacement, destination, 0);
}

/// Frees the fresh wrapper after its payload ownership was already transferred.
pub fn emit_retire_moved_box(emitter: &mut impl Emitter, replacement: &str) {
    emitter.emit_reg_move(emitter.int_result_reg(), replacement);
    emitter.emit_call_label("__rt_heap_free");
}
