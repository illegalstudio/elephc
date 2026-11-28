//! Purpose:
//! Defines the stack layout constants for EIR exception-handler slots.
//! Keeps try/catch frame offsets available to codegen and runtime emitters.
//!
//! Called from:
//! - `crate::codegen::frame` when reserving handler slots.
//! - `crate::codegen::lower_inst` when writing handler metadata.
//!
//! Key details:
//! - Offsets must stay synchronized with the runtime exception handler ABI.
//! - Every internal boundary pairs `_setjmp` with `_longjmp`, without saving signal masks.

use crate::codegen_support::emit::Emitter;

/// Size of the pre-allocated try handler slot.
pub(crate) use elephc_memoir::layout::TRY_HANDLER_SLOT_SIZE;

/// Offset within the try handler slot for the diagnostic depth field.
pub(crate) use elephc_memoir::layout::TRY_HANDLER_DIAG_DEPTH_OFFSET;

/// Offset within the try handler slot for the `jmp_buf` field.
pub(crate) use elephc_memoir::layout::TRY_HANDLER_JMP_BUF_OFFSET;

pub(crate) use elephc_memoir::activation_records::{
    GUARD_BYTES as EXCEPTION_GUARD_SLOT_SIZE,
    GUARD_OWNER as EXCEPTION_GUARD_OWNER_OFFSET,
};

impl Emitter {
    /// Saves an internal exception boundary without reading the signal mask.
    ///
    /// The caller supplies the jump-buffer pointer in C argument zero. All supported
    /// libcs expose `_setjmp` and its matching `_longjmp`; `bl_c` handles symbol spelling.
    /// PCNTL dispatch restores its own mask before the runtime resumes a handler.
    pub(crate) fn save_exception_handler(&mut self) {
        self.bl_c("_setjmp");
    }

    /// Resumes an internal boundary without overwriting deliberate signal-mask changes.
    ///
    /// The caller supplies the jump-buffer pointer and nonzero result in C arguments
    /// zero and one, after restoring PCNTL dispatch state and unwinding owned values.
    pub(crate) fn resume_exception_handler(&mut self) {
        self.bl_c("_longjmp");
    }
}
