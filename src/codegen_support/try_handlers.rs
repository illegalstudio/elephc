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

/// Size of the pre-allocated try handler slot.
pub(crate) const TRY_HANDLER_SLOT_SIZE: usize = 224;

/// Offset within the try handler slot for the diagnostic depth field.
pub(crate) const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;

/// Offset within the try handler slot for the `jmp_buf` field.
pub(crate) const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

/// Bytes in an owned-value cleanup activation, including its standard three-word prefix.
pub(crate) const EXCEPTION_GUARD_SLOT_SIZE: usize = 32;

/// Offset of the transferred heap owner after the cleanup activation prefix.
pub(crate) const EXCEPTION_GUARD_OWNER_OFFSET: usize = 24;
