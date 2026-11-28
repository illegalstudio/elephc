//! Purpose:
//! Installs and restores frame-local native exception boundaries around resumable cleanup.
//!
//! Called from:
//! - Callback-driven array helpers and callable invokers that retain temporary owners.
//!
//! Key details:
//! - The caller chooses frame slots and the caught label, while Memoir owns handler geometry.
//! - The active exception is parked before a new handler becomes visible.
//! - Restoration includes the caller's handler link and diagnostic-suppression depth.

use crate::{FrameOffset, Emitter, abi};
use crate::layout::TRY_HANDLER_JMP_BUF_OFFSET;

/// One frame-local cleanup boundary whose handler record lives in caller storage.
#[derive(Clone, Copy)]
pub struct CleanupBoundary {
    handler: FrameOffset,
}

impl CleanupBoundary {
    /// Selects the first byte of the caller's complete try-handler record.
    pub const fn new(handler: FrameOffset) -> Self {
        Self { handler }
    }

    /// Parks and clears the active exception owner before publishing this boundary.
    pub fn emit_park_exception(
        self,
        emitter: &mut impl Emitter,
        previous: FrameOffset,
    ) {
        let result = abi::int_result_reg(emitter);
        abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
        abi::store_at_offset(emitter, result, previous.bytes());
        abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    }

    /// Saves and publishes the handler record, then branches when native unwinding returns here.
    pub fn emit_install(self, emitter: &mut impl Emitter, caught: &str) {
        let result = abi::int_result_reg(emitter);
        crate::activation_records::emit_save_frame_try_prefix(
            emitter,
            result,
            self.handler.bytes(),
        );
        abi::emit_frame_slot_address(emitter, result, self.handler.bytes());
        crate::activation_records::emit_publish_try_handler(emitter, result);
        abi::emit_frame_slot_address(
            emitter,
            abi::int_arg_reg_name(emitter, 0),
            self.handler.bytes() - TRY_HANDLER_JMP_BUF_OFFSET,
        );
        emitter.save_exception_handler();                                      // retain native cleanup control across a throwing callback
        abi::emit_branch_if_int_result_nonzero(emitter, caught);
    }

    /// Parks the active exception and installs the complete boundary in one operation.
    pub fn emit_install_with_previous(
        self,
        emitter: &mut impl Emitter,
        previous: FrameOffset,
        caught: &str,
    ) {
        self.emit_park_exception(emitter, previous);
        self.emit_install(emitter, caught);
    }

    /// Restores the caller's handler link and diagnostic-suppression depth.
    pub fn emit_restore(self, emitter: &mut impl Emitter) {
        let result = abi::int_result_reg(emitter);
        crate::activation_records::emit_restore_frame_try_handler(
            emitter,
            result,
            self.handler.bytes(),
        );
    }
}
