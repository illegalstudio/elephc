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

/// Size of the pre-allocated try handler slot, aligned for every supported ABI.
/// glibc AArch64 needs 312 bytes for jmp_buf: 176 bytes of register storage,
/// an aligned mask flag, and a 128-byte signal set. Darwin arm64 needs 192 and
/// glibc x86_64 needs 200. Reserve the full largest object, not only the register
/// prefix currently written by a particular libc implementation.
pub(crate) const TRY_HANDLER_SLOT_SIZE: usize = (TRY_HANDLER_JMP_BUF_OFFSET + 312 + 15) & !15;

/// Offset within the try handler slot for the diagnostic depth field.
pub(crate) const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;

/// Offset within the try handler slot for the `jmp_buf` field.
pub(crate) const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

/// Bytes in an owned-value cleanup activation, including its standard three-word prefix.
pub(crate) const EXCEPTION_GUARD_SLOT_SIZE: usize = 32;

/// Offset of the pure owner visitor in extended PHP and call-operand activation records.
/// Word 5 (offset 40) holds the caller-file descriptor used by backtraces, so the visitor is
/// the last word of a 56-byte record.
pub(crate) const ACTIVATION_OWNER_VISITOR_OFFSET: usize = 48;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_handler_slot_fits_supported_libc_jump_buffers() {
        // glibc 2.39: sysdeps/aarch64/bits/setjmp.h defines 22 u64 words;
        // setjmp/bits/types/struct___jmp_buf_tag.h adds a mask flag and
        // the 128-byte signal set, with the C ABI's alignment padding.
        #[repr(C)]
        struct LinuxAarch64JmpBuf {
            registers: [u64; 22],
            mask_was_saved: i32,
            saved_mask: [u64; 16],
        }
        let linux_arm64_size = std::mem::size_of::<LinuxAarch64JmpBuf>();
        assert_eq!(linux_arm64_size, 312);
        for (target, size) in [
            ("macos-aarch64", 192),
            ("ios-arm64", 192),
            ("ios-sim-arm64", 192),
            ("linux-aarch64", linux_arm64_size),
            ("linux-x86_64", 200),
        ] {
            assert!(TRY_HANDLER_JMP_BUF_OFFSET + size <= TRY_HANDLER_SLOT_SIZE,
                "{target}: jump buffer exceeds its handler slot");
        }
        assert_eq!(TRY_HANDLER_SLOT_SIZE % 16, 0);
        assert!(TRY_HANDLER_DIAG_DEPTH_OFFSET + 8 <= TRY_HANDLER_JMP_BUF_OFFSET);
    }
}
