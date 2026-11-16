//! Purpose:
//! Tracks exception ownership and nested GC suppression throughout one deep heap release.
//!
//! Called from:
//! - Array, hash, Mixed, object, and callable cleanup, including coroutine storage branches.
//!
//! Key details:
//! - All participating cleanup frames reserve 64 bytes including frame linkage.
//! - Child cleanup returns normally; only the completed outer frame propagates its pending throw.

use crate::{abi, Emitter, Arch};

/// Locates the pending raw Throwable owner within the active target-specific cleanup frame.
pub fn pending_offset(emitter: &impl Emitter) -> usize {
    match emitter.arch() { Arch::AArch64 => 16, Arch::X86_64 => 40 }
}

/// Locates the enclosing GC suppression value without disturbing existing object-loop slots.
pub fn suppression_offset(emitter: &impl Emitter) -> usize {
    match emitter.arch() { Arch::AArch64 => 8, Arch::X86_64 => 48 }
}

/// Initializes pending ownership and suppresses collection without discarding an outer suppression.
pub fn begin(emitter: &mut impl Emitter) {
    let result = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_gc_release_suppressed", 0);
    abi::store_at_offset(emitter, result, suppression_offset(emitter));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_release_suppressed", 0);
}

/// Invokes one potentially throwing unary cleanup while preserving this frame's pending owner.
pub fn invoke(emitter: &mut impl Emitter, entry: &str, payload: &str) {
    emitter.emit_guarded_cleanup_call(entry, payload, pending_offset(emitter));
}

/// Restores GC state and returns or propagates only after every child and the container are freed.
pub fn finish(emitter: &mut impl Emitter, return_label: &str) {
    let result = abi::int_result_reg(emitter);
    let older = match emitter.arch() { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::load_at_offset(emitter, result, suppression_offset(emitter));
    abi::emit_store_reg_to_symbol(emitter, result, "_gc_release_suppressed", 0);
    abi::load_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_branch_if_int_result_zero(emitter, return_label);
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, pending_offset(emitter));
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(return_label);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}
