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

use crate::{Emitter, FrameOffset, abi};

/// Symbol and frame slot for a saved active stream descriptor.
#[derive(Clone, Copy)]
pub struct StreamScope<'a> {
    /// Global slot holding the currently active descriptor.
    pub active: &'a str,
    /// Local slot holding the descriptor to restore.
    pub saved: FrameOffset,
}

impl StreamScope<'_> {
    /// Installs a new descriptor at the scratch tail, keeping the outer descriptor.
    pub fn emit_enter(self, emitter: &mut impl Emitter, descriptor: FrameOffset) {
        let result = abi::int_result_reg(emitter);
        let arg0 = abi::int_arg_reg_name(emitter, 0);
        abi::emit_load_symbol_to_reg(emitter, result, self.active, 0);
        abi::store_at_offset(emitter, result, self.saved.bytes());
        abi::emit_frame_slot_address(emitter, arg0, descriptor.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_init");
        abi::emit_frame_slot_address(emitter, result, descriptor.bytes());
        abi::emit_store_reg_to_symbol(emitter, result, self.active, 0);
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
        abi::emit_load_symbol_to_reg(emitter, desc, self.active, 0);
        abi::store_at_offset(emitter, desc, self.saved.bytes());
        abi::emit_call_label(emitter, "__rt_strbuf_sync");
    }

    /// Restores the descriptor saved by entry or synchronization.
    pub fn emit_restore(self, emitter: &mut impl Emitter) {
        let result = abi::int_result_reg(emitter);
        abi::load_at_offset(emitter, result, self.saved.bytes());
        abi::emit_store_reg_to_symbol(emitter, result, self.active, 0);
    }
}
