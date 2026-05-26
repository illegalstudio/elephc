//! Purpose:
//! Emits PHP `ftell` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits PHP `ftell()` which returns the current file position offset.
///
/// # Arguments
/// - `args[0]`: PHP stream resource to query.
///
/// # Behavior
/// Unboxes the stream resource to extract its file descriptor, then issues a
/// `lseek(fd, 0, SEEK_CUR)` syscall/libc call to retrieve the current position.
/// Returns `i64` (PhpType::Int) representing bytes from the start of the file.
///
/// # ABI (ARM64)
/// - `emit_stream_fd_arg` places the fd in `x0`.
/// - `lseek` syscall (#199) uses `x0`=fd, `x1`=offset(0), `x2`=whence(SEEK_CUR=1).
/// - Result returned in `x0`.
///
/// # ABI (x86_64)
/// - `emit_stream_fd_arg` leaves fd in `rax` after stream unboxing.
/// - Libc `lseek(rdi, rsi, rdx)` uses `rdi`=fd, `rsi`=offset(0), `rdx`=whence(SEEK_CUR=1).
/// - Result returned in `rax`.
///
/// # PHP semantics
/// - Returns `false` on error (invalid stream, not seekable). Codegen does not
///   model PHP error/false propagation here — caller handles type/warnings.
/// - Position is a non-negative integer; -1 indicates error.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ftell()");
    emit_stream_fd_arg("ftell", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, #0");                                  // offset = 0 for the AArch64 ftell() lseek syscall
            emitter.instruction("mov x2, #1");                                  // whence = SEEK_CUR for the AArch64 ftell() lseek syscall
            emitter.syscall(199);                                               // ask the kernel for the current file position through lseek()
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV lseek() argument register
            emitter.instruction("xor esi, esi");                                // offset = 0 for the linux-x86_64 ftell() lseek() call
            emitter.instruction("mov edx, 1");                                  // whence = SEEK_CUR for the linux-x86_64 ftell() lseek() call
            emitter.instruction("call lseek");                                  // ask libc lseek() for the current file position on linux-x86_64
        }
    }
    Some(PhpType::Int)
}
