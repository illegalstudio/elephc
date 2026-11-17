//! Purpose:
//! Transfers an owner out of a frame slot before calling cleanup.
//!
//! Called from:
//! - Compiler emitters for array callbacks and owned callable arguments.
//!
//! Key details:
//! - Clearing the slot before destruction makes repeated cleanup safe.
//! - The caller supplies the release entry and its surrounding exception handling.

use crate::{Emitter, FrameOffset, TargetAbi};

/// Clears a pointer-sized owner slot without disturbing the native result register.
pub fn emit_clear(emitter: &mut impl TargetAbi, offset: FrameOffset) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_load_int_immediate(scratch, 0);
    emitter.store_at_offset(scratch, offset.bytes());
}

/// Takes the owner and calls a unary native release entry under the caller's handler.
pub fn emit_release(emitter: &mut impl TargetAbi, offset: FrameOffset, entry: &str) {
    emit_take(emitter, offset);
    emitter.emit_call_label(entry);
}

/// Takes the owner before entering bounded cleanup with a caller-owned pending slot.
pub fn emit_guarded_release(
    emitter: &mut impl Emitter,
    offset: FrameOffset,
    entry: &str,
    pending: FrameOffset,
) {
    emit_take(emitter, offset);
    emitter.emit_guarded_cleanup_call(entry, emitter.int_result_reg(), pending.bytes());
}

/// Transfers the owner to the native result register and clears its former slot.
fn emit_take(emitter: &mut impl TargetAbi, offset: FrameOffset) {
    emitter.load_at_offset(emitter.int_result_reg(), offset.bytes());
    emit_clear(emitter, offset);
}
