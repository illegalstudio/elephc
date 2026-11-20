//! Purpose:
//! Defines the native activation chain layout and publishes ordinary frame records.
//!
//! Called from:
//! - Compiler frame emission, owner records, exceptional cleanup and collector traversal.
//!
//! Key details:
//! - Every record has the same three-word unwind prefix.
//! - Extended records share the visitor field but can use the middle words differently.

use crate::{Arch, AssemblyEmitter, FrameOffset, TargetAbi};

/// Offset of the previous activation link in the shared prefix.
pub const PREVIOUS: usize = 0;
/// Offset of the callback invoked while unwinding the activation.
pub const CLEANUP: usize = 8;
/// Offset of the callback's data, such as a frame pointer or an owner slot.
pub const DATA: usize = 16;
/// Size of the prefix shared by compact and extended activations.
pub const PREFIX_BYTES: usize = DATA + 8;
/// Offset of the one transferred owner after the prefix of a compact guard.
pub const GUARD_OWNER: usize = PREFIX_BYTES;
/// Size of a compact transferred-owner guard.
pub const GUARD_BYTES: usize = GUARD_OWNER + 8;
/// Offset of the optional native backtrace reader in an ordinary frame.
pub const FRAME_READER: usize = PREFIX_BYTES;
/// Offset of the caller's source line in an ordinary frame.
pub const FRAME_CALLER_LINE: usize = FRAME_READER + 8;
/// Offset of the saved temporary stack base in an extended operand activation.
pub const OPERAND_STACK_BASE: usize = PREFIX_BYTES + 8;
/// Offset of the pure GC owner visitor shared by all extended activations.
pub const OWNER_VISITOR: usize = PREFIX_BYTES + 16;
/// Size of an ordinary frame or another extended owner activation.
pub const EXTENDED_BYTES: usize = OWNER_VISITOR + 8;

/// Saves the three values that a native exception rewind record must restore.
pub fn emit_save_try_prefix<E: TargetAbi>(
    emitter: &mut E,
    scratch: &str,
    mut store: impl FnMut(&mut E, &str, usize),
) {
    for (symbol, offset) in [
        ("_exc_handler_top", 0),
        ("_exc_call_frame_top", 8),
        ("_rt_diag_suppression", crate::layout::TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        emitter.emit_load_symbol_to_reg(scratch, symbol, 0);
        store(emitter, scratch, offset);
    }
}

/// Saves a rewind prefix in a native frame addressed by a positive fp displacement.
pub fn emit_save_frame_try_prefix(emitter: &mut impl TargetAbi, scratch: &str, offset: usize) {
    emit_save_try_prefix(emitter, scratch, |emitter, source, field| {
        emitter.store_at_offset(source, offset - field);
    });
}

/// Saves a rewind prefix relative to the current stack pointer.
pub fn emit_save_sp_try_prefix(emitter: &mut impl TargetAbi, scratch: &str, offset: usize) {
    emit_save_try_prefix(emitter, scratch, |emitter, source, field| {
        emitter.emit_store_to_sp(source, offset + field);
    });
}

/// Saves a rewind prefix in an AArch64 runtime frame with explicit stack displacements.
pub fn emit_save_arm_runtime_try_prefix(emitter: &mut impl crate::Emitter, scratch: &str) {
    emit_save_try_prefix(emitter, scratch, |emitter, source, field| {
        emitter.instruction(&format!("str {source}, [sp, #{field}]"));          // save the rewind record field
    });
}

/// Saves a rewind prefix at a positive displacement from the AArch64 frame pointer.
pub fn emit_save_arm_fp_try_prefix(emitter: &mut impl crate::Emitter, scratch: &str, offset: usize) {
    emit_save_try_prefix(emitter, scratch, |emitter, source, field| {
        emitter.instruction(&format!("str {source}, [x29, #{}]", offset + field)); // save the rewind record field
    });
}

/// Saves a rewind prefix at a record address already materialized by the caller.
pub fn emit_save_addressed_try_prefix(emitter: &mut impl TargetAbi, scratch: &str, base: &str) {
    emit_save_try_prefix(emitter, scratch, |emitter, source, field| {
        emitter.emit_store_to_address(source, base, field);
    });
}

/// Publishes the address of the initialized rewind record.
pub fn emit_publish_try_handler(emitter: &mut impl TargetAbi, address: &str) {
    emitter.emit_store_reg_to_symbol(address, "_exc_handler_top", 0);
}

/// Updates the owner held by a detached or active guard after its value relocates.
pub fn emit_update_guard_owner(emitter: &mut impl TargetAbi, record: &str, owner: &str) {
    emitter.emit_store_to_address(owner, record, GUARD_OWNER);
}

/// Optional symbols selected by the compiler for one ordinary frame activation.
pub struct FrameActivation<'a> {
    /// Writable symbol holding the currently active frame.
    pub head: &'a str,
    /// Callback for destroying abandoned frame owners.
    pub cleanup: Option<&'a str>,
    /// Callback for materializing the frame's PHP backtrace entry.
    pub reader: Option<&'a str>,
    /// Source of the incoming backtrace coordinate, when source tracing is enabled.
    pub caller_line: Option<&'a str>,
    /// Pure callback for visiting the frame's owners when the stack is suspended.
    pub visitor: Option<&'a str>,
}

/// Initializes the frame's activation record before publishing it onto the chain.
pub fn emit_push_frame<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, offset: FrameOffset, activation: FrameActivation<'_>,
) {
    emitter.comment("publish PHP frame activation");
    let (scratch, frame_pointer) = match emitter.arch() {
        Arch::AArch64 => ("x10", "x29"),
        Arch::X86_64 => ("r10", "rbp"),
    };
    let offset = offset.bytes();
    emitter.emit_load_symbol_to_reg(scratch, activation.head, 0);
    emitter.store_at_offset(scratch, offset - PREVIOUS);
    emit_optional_callback(emitter, scratch, activation.cleanup);
    emitter.store_at_offset(scratch, offset - CLEANUP);
    emitter.store_at_offset(frame_pointer, offset - DATA);
    emit_optional_callback(emitter, scratch, activation.reader);
    emitter.store_at_offset(scratch, offset - FRAME_READER);
    if let Some(source) = activation.caller_line {
        emitter.emit_load_symbol_to_reg(scratch, source, 0);
    }
    emitter.store_at_offset(scratch, offset - FRAME_CALLER_LINE);
    emit_optional_callback(emitter, scratch, activation.visitor);
    emitter.store_at_offset(scratch, offset - OWNER_VISITOR);
    emitter.emit_frame_slot_address(scratch, offset);
    emitter.emit_store_reg_to_symbol(scratch, activation.head, 0);
}

/// Loads a callback address or the null value used for an absent callback.
fn emit_optional_callback(emitter: &mut impl TargetAbi, scratch: &str, callback: Option<&str>) {
    if let Some(callback) = callback {
        emitter.emit_symbol_address(scratch, callback);
    } else {
        emitter.emit_load_int_immediate(scratch, 0);
    }
}

/// Removes the current ordinary frame by restoring the previous link.
pub fn emit_pop_frame(emitter: &mut impl TargetAbi, offset: FrameOffset, head: &str) {
    let scratch = match emitter.arch() {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    emitter.load_at_offset(scratch, offset.bytes() - PREVIOUS);
    emitter.emit_store_reg_to_symbol(scratch, head, 0);
}
