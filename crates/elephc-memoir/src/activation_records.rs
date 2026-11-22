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
use crate::owner_layout::ExtendedActivation;

/// Offset of the previous activation link in the shared prefix.
pub const PREVIOUS: usize = ExtendedActivation::PREVIOUS;
/// Offset of the callback invoked while unwinding the activation.
pub const CLEANUP: usize = ExtendedActivation::CLEANUP;
/// Offset of the callback's data, such as a frame pointer or an owner slot.
pub const DATA: usize = ExtendedActivation::DATA;
/// Size of the prefix shared by compact and extended activations.
pub const PREFIX_BYTES: usize = ExtendedActivation::EXTENSION;
/// Offset of the one transferred owner after the prefix of a compact guard.
pub const GUARD_OWNER: usize = PREFIX_BYTES;
/// Size of a compact transferred-owner guard.
pub const GUARD_BYTES: usize = GUARD_OWNER + 8;
/// Offset of the optional native backtrace reader in an ordinary frame.
pub const FRAME_READER: usize = PREFIX_BYTES;
/// Offset of the caller's source line in an ordinary frame.
pub const FRAME_CALLER_LINE: usize = ExtendedActivation::LINE_OR_STACK;
/// Offset of the saved temporary stack base in an extended operand activation.
pub const OPERAND_STACK_BASE: usize = ExtendedActivation::LINE_OR_STACK;
/// Offset of the pure GC owner visitor shared by all extended activations.
pub const OWNER_VISITOR: usize = ExtendedActivation::OWNER_VISITOR;
/// Size of an ordinary frame or another extended owner activation.
pub const EXTENDED_BYTES: usize = ExtendedActivation::BYTES;

/// A saved runtime word in the common native exception record.
#[derive(Clone, Copy)]
pub enum RewindField {
    /// Previous exception handler chain head.
    Handler,
    /// Surviving native activation.
    CallFrame,
    /// Diagnostic suppression depth at entry.
    DiagnosticDepth,
}

impl RewindField {
    /// Resolves the saved stack slot and the corresponding runtime global.
    fn location(self) -> (&'static str, usize) {
        match self {
            Self::Handler => ("_exc_handler_top", 0),
            Self::CallFrame => ("_exc_call_frame_top", 8),
            Self::DiagnosticDepth => ("_rt_diag_suppression", crate::layout::TRY_HANDLER_DIAG_DEPTH_OFFSET),
        }
    }
}

/// The complete rewind prefix in the order it is saved and restored.
pub const REWIND_FIELDS: [RewindField; 3] = [
    RewindField::Handler, RewindField::CallFrame, RewindField::DiagnosticDepth,
];

/// Fields restored when returning through a local callback boundary.
pub const CALLBACK_REWIND_FIELDS: [RewindField; 2] = [
    RewindField::Handler, RewindField::DiagnosticDepth,
];

/// Restores the selected fields in caller-supplied order from the given frame representation.
pub fn emit_restore_try_fields<E: TargetAbi>(
    emitter: &mut E, scratch: &str, fields: &[RewindField],
    mut load: impl FnMut(&mut E, &str, usize),
) {
    for field in fields {
        let (symbol, offset) = field.location();
        load(emitter, scratch, offset);
        emitter.emit_store_reg_to_symbol(scratch, symbol, 0);
    }
}

/// Restores the full rewind record at the bottom of a protected callback frame.
pub fn emit_restore_runtime_try_prefix(emitter: &mut impl crate::Emitter, scratch: &str) {
    emit_restore_try_fields(emitter, scratch, &REWIND_FIELDS, |emitter, register, offset| {
        match emitter.arch() {
            crate::Arch::AArch64 => emitter.instruction(&format!("ldr {register}, [sp, #{offset}]")), // restore the saved AArch64 rewind word
            crate::Arch::X86_64 => emitter.instruction(&format!("mov {register}, QWORD PTR [rsp + {offset}]")), // restore the saved SysV rewind word
        }
    });
}

/// Saves the full rewind record at the bottom of a protected callback frame.
pub fn emit_save_runtime_try_prefix(emitter: &mut impl crate::Emitter, scratch: &str) {
    emit_save_try_prefix(emitter, scratch, |emitter, register, offset| {
        match emitter.arch() {
            crate::Arch::AArch64 => emitter.instruction(&format!("str {register}, [sp, #{offset}]")), // save the AArch64 rewind word
            crate::Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], {register}")), // save the SysV rewind word
        }
    });
}

/// Installs the sentinel prefix used when an exception escapes a fresh coroutine.
pub fn emit_save_fiber_entry_prefix(emitter: &mut impl crate::Emitter, scratch: &str) {
    for field in REWIND_FIELDS {
        let (symbol, offset) = field.location();
        let base = if emitter.arch() == crate::Arch::AArch64 { "sp" } else { "rsp" };
        if matches!(field, RewindField::CallFrame) {
            emitter.emit_store_zero_to_address(base, offset);
        } else {
            emitter.emit_load_symbol_to_reg(scratch, symbol, 0);
            if emitter.arch() == crate::Arch::AArch64 {
                emitter.instruction(&format!("str {scratch}, [sp, #{offset}]")); // save the handler head or diagnostic depth
            } else {
                emitter.emit_store_to_address(scratch, base, offset);
            }
        }
    }
}

/// Restores the coroutine-entry handler and, on escape, its diagnostic depth.
pub fn emit_restore_fiber_entry_prefix(emitter: &mut impl crate::Emitter, scratch: &str, escaped: bool) {
    let fields = [RewindField::Handler, RewindField::DiagnosticDepth];
    let count = if escaped { 2 } else { 1 };
    emit_restore_try_fields(emitter, scratch, &fields[..count], |emitter, register, offset| {
        if emitter.arch() == crate::Arch::AArch64 {
            emitter.instruction(&format!("ldr {register}, [sp, #{offset}]"));   // reload the saved coroutine rewind word
        } else {
            emitter.emit_load_from_address(register, "rsp", offset);
        }
    });
}

/// Saves the three values that a native exception rewind record must restore.
pub fn emit_save_try_prefix<E: TargetAbi>(
    emitter: &mut E,
    scratch: &str,
    mut store: impl FnMut(&mut E, &str, usize),
) {
    for field in REWIND_FIELDS {
        let (symbol, offset) = field.location();
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

/// Unlinks a callback boundary stored at a negative frame-pointer displacement.
pub fn emit_restore_frame_try_handler(emitter: &mut impl TargetAbi, scratch: &str, offset: usize) {
    emit_restore_try_fields(emitter, scratch, &CALLBACK_REWIND_FIELDS, |emitter, register, field| {
        emitter.load_at_offset(register, offset - field);
    });
}

/// Unlinks a callback boundary stored at a positive AArch64 frame-pointer displacement.
pub fn emit_restore_arm_fp_try_handler(emitter: &mut impl crate::Emitter, scratch: &str, offset: usize) {
    emit_restore_try_fields(emitter, scratch, &CALLBACK_REWIND_FIELDS, |emitter, register, field| {
        emitter.instruction(&format!("ldr {register}, [x29, #{}]", offset + field)); // reload the saved callback rewind word
    });
}

/// Unlinks a callback boundary stored relative to the current stack pointer.
pub fn emit_restore_sp_try_handler(emitter: &mut impl crate::Emitter, scratch: &str, offset: usize) {
    emit_restore_try_fields(emitter, scratch, &CALLBACK_REWIND_FIELDS, |emitter, register, field| {
        match emitter.arch() {
            Arch::AArch64 => emitter.instruction(&format!("ldr {register}, [sp, #{}]", offset + field)), // reload the saved ARM64 callback rewind word
            Arch::X86_64 => emitter.instruction(&format!("mov {register}, QWORD PTR [rsp + {}]", offset + field)), // reload the saved SysV callback rewind word
        }
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
