//! Purpose:
//! Emits PHP `ftell` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::abi;
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
    let user_wrapper_label = ctx.next_label("ftell_user_wrapper");
    let after_dispatch = ctx.next_label("ftell_after_dispatch");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- user-wrapper synthetic fd path (Phase 10 step 4) --
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", user_wrapper_label));       // dispatch into the wrapper's stream_tell instead of lseek
            emitter.instruction("mov x1, #0");                                  // offset = 0 for the AArch64 ftell() lseek syscall
            emitter.instruction("mov x2, #1");                                  // whence = SEEK_CUR for the AArch64 ftell() lseek syscall
            emitter.syscall(199);                                               // ask the kernel for the current file position through lseek()
            emitter.instruction(&format!("b {}", after_dispatch));              // skip the user-wrapper path on the normal-fd success/failure
            emitter.label(&user_wrapper_label);
            abi::emit_call_label(emitter, "__rt_user_wrapper_ftell");           // dispatch into the wrapper's stream_tell
            emitter.label(&after_dispatch);
        }
        Arch::X86_64 => {
            // -- user-wrapper synthetic fd path (Phase 10 step 4) --
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", user_wrapper_label));        // dispatch into the wrapper's stream_tell instead of lseek
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV lseek() argument register
            emitter.instruction("xor esi, esi");                                // offset = 0 for the linux-x86_64 ftell() lseek() call
            emitter.instruction("mov edx, 1");                                  // whence = SEEK_CUR for the linux-x86_64 ftell() lseek() call
            emitter.instruction("call lseek");                                  // ask libc lseek() for the current file position on linux-x86_64
            emitter.instruction(&format!("jmp {}", after_dispatch));            // skip the user-wrapper path on the normal-fd success/failure
            emitter.label(&user_wrapper_label);
            emitter.instruction("mov rdi, rax");                                // move the synthetic fd into the first SysV arg register for the wrapper helper
            abi::emit_call_label(emitter, "__rt_user_wrapper_ftell");           // dispatch into the wrapper's stream_tell
            emitter.label(&after_dispatch);
        }
    }
    Some(PhpType::Int)
}
