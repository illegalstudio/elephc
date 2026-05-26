//! Purpose:
//! Emits PHP `fclose` stream builtin calls over runtime file handles.
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

/// Emits PHP `fclose(stream)` by extracting the file descriptor from the stream
/// resource, closing it via the target syscall/libc, and returning a bool indicating
/// success (true) or failure (false). Consumes the stream resource in `args[0]`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fclose()");
    emit_stream_fd_arg("fclose", &args[0], emitter, ctx, data);
    let success = ctx.next_label("fclose_ok");
    let done = ctx.next_label("fclose_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.syscall(6);                                                 // close the requested file descriptor through the platform syscall path
            emitter.instruction("cmp x0, #0");                                  // did the close syscall report success?
            emitter.instruction(&format!("b.eq {}", success));                  // branch to the success result when the close syscall returns zero
            emitter.instruction("mov x0, #0");                                  // return false when the close syscall reports an error
            emitter.instruction(&format!("b {}", done));                        // skip the success result write on the error path
            emitter.label(&success);
            emitter.instruction("mov x0, #1");                                  // return true when the close syscall succeeds
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV libc close() argument register
            emitter.instruction("call close");                                  // close the requested file descriptor through libc close()
            emitter.instruction("cmp rax, 0");                                  // did libc close() report success?
            emitter.instruction(&format!("je {}", success));                    // branch to the success result when libc close() returns zero
            emitter.instruction("xor eax, eax");                                // return false when libc close() reports an error
            emitter.instruction(&format!("jmp {}", done));                      // skip the success result write on the error path
            emitter.label(&success);
            emitter.instruction("mov rax, 1");                                  // return true when libc close() succeeds
        }
    }
    emitter.label(&done);
    Some(PhpType::Bool)
}
