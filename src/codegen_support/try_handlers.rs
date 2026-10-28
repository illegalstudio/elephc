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
pub(crate) const TRY_HANDLER_SLOT_SIZE: usize = 224;

/// Offset within the try handler slot for the diagnostic depth field.
pub(crate) const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;

/// Offset within the try handler slot for the `jmp_buf` field.
pub(crate) const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

/// Bytes in an owned-value cleanup activation, including its standard three-word prefix.
pub(crate) const EXCEPTION_GUARD_SLOT_SIZE: usize = 32;

/// Offset of the pure owner visitor in extended PHP and call-operand activation records.
pub(crate) const ACTIVATION_OWNER_VISITOR_OFFSET: usize = 40;

/// Offset of the transferred heap owner after the cleanup activation prefix.
pub(crate) const EXCEPTION_GUARD_OWNER_OFFSET: usize = 24;

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
