//! Purpose:
//! Emits PHP `rewind` stream builtin calls over runtime file handles.
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
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for the PHP `rewind()` builtin.
///
/// Unboxes the stream resource from `args[0]` to extract its file descriptor,
/// then calls the platform lseek routine with offset=0 and whence=SEEK_SET to
/// reset the file pointer to the start of the stream. On success, clears the
/// EOF flag for the file descriptor. On failure, returns false without modifying
/// the stream state.
///
/// # Arguments
/// - `_name`: Ignored; present for dispatcher uniformity.
/// - `args`: Must contain exactly one expression resolving to a stream resource.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context providing labels, frame layout, and platform details.
/// - `data`: Data section for literals and global symbol addresses.
///
/// # Returns
/// Always returns `Some(PhpType::Bool)` — `true` on success, `false` on failure.
///
/// # Platform details
/// - **AArch64**: Uses syscall 199 (`lseek`), preserves the fd across the call via
///   stack push/pop, and clears `_eof_flags[x9]` on success.
/// - **x86_64**: Calls libc `lseek()`, preserves the fd across the call via stack
///   push/pop, and clears `_eof_flags[r10]` on success.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rewind()");
    emit_stream_fd_arg("rewind", &args[0], emitter, ctx, data);
    let success_label = ctx.next_label("rewind_success");
    let done_label = ctx.next_label("rewind_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0");                                  // preserve fd so successful rewind() can clear its EOF flag
            emitter.instruction("mov x1, #0");                                  // offset = 0 for the AArch64 rewind() lseek syscall
            emitter.instruction("mov x2, #0");                                  // whence = SEEK_SET for the AArch64 rewind() lseek syscall
            emitter.syscall(199);                                               // reset the file position through the platform lseek syscall path
            if emitter.platform.needs_cmp_before_error_branch() {
                emitter.instruction("cmp x0, #0");                              // Linux: negative lseek result means rewind() failed
            }
            emitter.instruction(&emitter.platform.branch_on_syscall_success(&success_label)); // continue only when lseek succeeded
            abi::emit_pop_reg(emitter, "x9");                                   // discard preserved fd on the rewind() failure path
            emitter.instruction("mov x0, #0");                                  // rewind() returns false when lseek fails
            emitter.instruction(&format!("b {}", done_label));                  // skip EOF reset after a failed seek
            emitter.label(&success_label);
            abi::emit_pop_reg(emitter, "x9");                                   // restore fd for EOF-flag reset after a successful seek
            abi::emit_symbol_address(emitter, "x10", "_eof_flags");
            emitter.instruction("strb wzr, [x10, x9]");                         // clear EOF because rewind() moved the stream back to the start
            emitter.instruction("mov x0, #1");                                  // rewind() returns true after a successful seek
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV lseek() argument register
            abi::emit_push_reg(emitter, "rdi");                                 // preserve fd so successful rewind() can clear its EOF flag
            emitter.instruction("xor esi, esi");                                // offset = 0 for the linux-x86_64 rewind() lseek() call
            emitter.instruction("xor edx, edx");                                // whence = SEEK_SET for the linux-x86_64 rewind() lseek() call
            emitter.instruction("call lseek");                                  // reset the file position through libc lseek() on linux-x86_64
            emitter.instruction("cmp rax, 0");                                  // did libc lseek() succeed with a non-negative resulting offset?
            emitter.instruction(&format!("jge {}", success_label));             // continue only when rewind() succeeded
            abi::emit_pop_reg(emitter, "r10");                                  // discard preserved fd on the rewind() failure path
            emitter.instruction("xor eax, eax");                                // rewind() returns false when lseek fails
            emitter.instruction(&format!("jmp {}", done_label));                // skip EOF reset after a failed seek
            emitter.label(&success_label);
            abi::emit_pop_reg(emitter, "r10");                                  // restore fd for EOF-flag reset after a successful seek
            emitter.instruction("lea r11, [rip + _eof_flags]");                 // materialize the eof-flag table for rewind()
            emitter.instruction("mov BYTE PTR [r11 + r10], 0");                 // clear EOF because rewind() moved the stream back to the start
            emitter.instruction("mov rax, 1");                                  // rewind() returns true after a successful seek
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Bool)
}
