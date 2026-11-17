//! Purpose:
//! Detaches two owned string pairs from one row before freeing either payload.
//!
//! Called from:
//! - The compiler's stream registration retirement entry.
//!
//! Key details:
//! - Scratch storage holds the two detached pointers across release calls.

use crate::TargetAbi;

/// Clears a two-string row and releases both detached pointers.
pub fn emit_retire_row(emitter: &mut impl TargetAbi, row: &str, frame: &str, scratch: &str) {
    for (source, destination) in [(0, 0), (16, 8)] {
        emitter.emit_load_from_address(scratch, row, source);
        emitter.emit_store_to_address(scratch, frame, destination);
    }
    for offset in [0, 8, 16, 24] {
        emitter.emit_store_zero_to_address(row, offset);
    }
    // -- release each transferred owner after the row no longer publishes it --
    for offset in [0, 8] {
        emitter.emit_load_from_address(row, frame, offset);
        emitter.emit_call_label("__rt_heap_free_safe");
    }
}
