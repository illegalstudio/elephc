//! Purpose:
//! Manages a growable string stream installed in a nested emitter scope.
//!
//! Called from:
//! - Serialization boundaries and callbacks that borrow the active stream.
//!
//! Key details:
//! - Each scope saves the previous descriptor in its caller-owned frame.
//! - A closing scope either transfers its finished bytes or releases its buffer.
//! - Synchronizing a current stream protects scratch bytes across reentry.

use crate::{Arch, Emitter, FrameOffset, abi};

const ACTIVE_STREAM: &str = "_ser_stream";

/// Materializes the address of the active serializer stream slot.
pub fn emit_active_address(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_symbol_address(destination, ACTIVE_STREAM);
}

/// Loads the current serializer stream descriptor.
pub fn emit_load_active(emitter: &mut impl Emitter, destination: &str) {
    emitter.emit_load_symbol_to_reg(destination, ACTIVE_STREAM, 0);
}

/// Publishes the current serializer stream descriptor.
pub fn emit_store_active(emitter: &mut impl Emitter, source: &str) {
    emitter.emit_store_reg_to_symbol(source, ACTIVE_STREAM, 0);
}

/// Loads the allocation start from a borrowed string-buffer descriptor.
pub fn emit_load_start(emitter: &mut impl Emitter, destination: &str, descriptor: &str) {
    emitter.emit_load_from_address(destination, descriptor, super::strbuf::STRBUF_START);
}

/// Loads the write cursor from a borrowed string-buffer descriptor.
pub fn emit_load_cursor(emitter: &mut impl Emitter, destination: &str, descriptor: &str) {
    emitter.emit_load_from_address(destination, descriptor, super::strbuf::STRBUF_WRITE);
}

/// Commits a write cursor to a borrowed string-buffer descriptor.
pub fn emit_store_cursor(emitter: &mut impl Emitter, source: &str, descriptor: &str) {
    emitter.emit_store_to_address(source, descriptor, super::strbuf::STRBUF_WRITE);
}

/// Symbol and frame slot for a saved active stream descriptor.
#[derive(Clone, Copy)]
pub struct StreamScope {
    /// Local slot holding the descriptor to restore.
    pub saved: FrameOffset,
}

impl StreamScope {
    /// Installs a new descriptor at the scratch tail, keeping the outer descriptor.
    pub fn emit_enter(self, emitter: &mut impl Emitter, descriptor: FrameOffset) {
        let result = abi::int_result_reg(emitter);
        let arg0 = abi::int_arg_reg_name(emitter, 0);
        emit_load_active(emitter, result);
        abi::store_at_offset(emitter, result, self.saved.bytes());
        abi::emit_frame_slot_address(emitter, arg0, descriptor.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_init");
        abi::emit_frame_slot_address(emitter, result, descriptor.bytes());
        emit_store_active(emitter, result);
    }

    /// Closes the current stream on either path and restores the outer descriptor.
    /// Finishing transfers the returned pointer and length into the supplied frame slots.
    pub fn emit_close(
        self,
        emitter: &mut impl Emitter,
        descriptor: FrameOffset,
        pending: FrameOffset,
        output: [FrameOffset; 2],
        finish: &str,
        closed: &str,
    ) {
        let result = abi::int_result_reg(emitter);
        let arg0 = abi::int_arg_reg_name(emitter, 0);
        abi::load_at_offset(emitter, result, pending.bytes());
        abi::emit_branch_if_int_result_zero(emitter, finish);
        abi::emit_frame_slot_address(emitter, arg0, descriptor.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_release");
        abi::emit_jump(emitter, closed);
        emitter.label(finish);
        abi::emit_frame_slot_address(emitter, arg0, descriptor.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_finish_owned");
        let (pointer, length) = abi::string_result_regs(emitter);
        abi::store_at_offset(emitter, pointer, output[0].bytes());
        abi::store_at_offset(emitter, length, output[1].bytes());
        emitter.label(closed);
        self.emit_restore(emitter);
    }

    /// Saves the active stream and publishes the scratch cursor before reentry.
    pub fn emit_save_and_sync(self, emitter: &mut impl Emitter) {
        let desc = abi::int_arg_reg_name(emitter, 0);
        emit_load_active(emitter, desc);
        abi::store_at_offset(emitter, desc, self.saved.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_sync");
    }

    /// Restores the descriptor saved by entry or synchronization.
    pub fn emit_restore(self, emitter: &mut impl Emitter) {
        let result = abi::int_result_reg(emitter);
        abi::load_at_offset(emitter, result, self.saved.bytes());
        emit_store_active(emitter, result);
    }
}

/// Emits reserve, tell, and seek helpers for the active serializer stream.
pub fn emit_serializer_helpers(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: serialize output stream ---");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.label_global("__rt_ser_reserve");
            emit_load_active(emitter, "x0");
            emitter.instruction("b __rt_strbuf_ensure");                        // grow when the requested bytes exceed capacity

            emitter.label_global("__rt_ser_tell");
            emit_load_active(emitter, "x9");
            emit_load_cursor(emitter, "x10", "x9");
            emit_load_start(emitter, "x9", "x9");
            emitter.instruction("sub x0, x10, x9");                             // bytes written so far
            emitter.instruction("ret");                                         // return the stream offset

            emitter.label_global("__rt_ser_seek");
            emit_load_active(emitter, "x9");
            emit_load_start(emitter, "x10", "x9");
            emitter.instruction("add x10, x10, x0");                            // cursor at the requested offset
            emit_store_cursor(emitter, "x10", "x9");
            emitter.instruction("ret");                                         // return to the serializer
        }
        Arch::X86_64 => {
            emitter.label_global("__rt_ser_reserve");
            emit_load_active(emitter, "rdi");
            emitter.instruction("jmp __rt_strbuf_ensure");                      // grow when the requested bytes exceed capacity

            emitter.label_global("__rt_ser_tell");
            emit_load_active(emitter, "r8");
            emit_load_cursor(emitter, "rax", "r8");
            emit_load_start(emitter, "r9", "r8");
            emitter.instruction("sub rax, r9");                                 // bytes written so far
            emitter.instruction("ret");                                         // return the stream offset

            emitter.label_global("__rt_ser_seek");
            emit_load_active(emitter, "r8");
            emit_load_start(emitter, "r9", "r8");
            emitter.instruction("add rax, r9");                                 // cursor at the requested offset
            emit_store_cursor(emitter, "rax", "r8");
            emitter.instruction("ret");                                         // return to the serializer
        }
    }
}
