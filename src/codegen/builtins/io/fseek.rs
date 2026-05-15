//! Purpose:
//! Emits PHP `fseek` stream builtin calls over runtime file handles.
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
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fseek()");
    emit_stream_fd_arg("fseek", &args[0], emitter, ctx, data);
    let success_label = ctx.next_label("fseek_success");
    let done_label = ctx.next_label("fseek_done");
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the seek offset expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the seek offset while the optional whence expression is evaluated
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // default whence = SEEK_SET for the AArch64 lseek path
            }
            Arch::X86_64 => {
                emitter.instruction("xor eax, eax");                            // default whence = SEEK_SET for the x86_64 lseek path
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, x0");                                  // move the whence selector into the third lseek syscall argument register
            abi::emit_pop_reg(emitter, "x1");                                   // restore the seek offset into the second lseek syscall argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the first lseek syscall argument register
            abi::emit_push_reg(emitter, "x0");                                  // preserve fd so successful fseek() can clear its EOF flag
            emitter.syscall(199);                                               // reposition the file offset through the platform syscall path
            if emitter.platform.needs_cmp_before_error_branch() {
                emitter.instruction("cmp x0, #0");                              // Linux: negative lseek result means fseek() failed
            }
            emitter.instruction(&emitter.platform.branch_on_syscall_success(&success_label)); // continue only when lseek succeeded
            abi::emit_pop_reg(emitter, "x9");                                   // discard preserved fd on the fseek() failure path
            emitter.instruction("mov x0, #-1");                                 // fseek() returns -1 on failure
            emitter.instruction(&format!("b {}", done_label));                  // skip EOF reset after a failed seek
            emitter.label(&success_label);
            abi::emit_pop_reg(emitter, "x9");                                   // restore fd for EOF-flag reset after a successful seek
            abi::emit_symbol_address(emitter, "x10", "_eof_flags");
            emitter.instruction("strb wzr, [x10, x9]");                         // clear EOF because fseek() repositioned the stream
            emitter.instruction("mov x0, #0");                                  // fseek() returns 0 on success
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // move the whence selector into the third SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the seek offset into the second SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV lseek() argument register
            abi::emit_push_reg(emitter, "rdi");                                 // preserve fd so successful fseek() can clear its EOF flag
            emitter.instruction("call lseek");                                  // reposition the file offset through libc lseek() on linux-x86_64
            emitter.instruction("cmp rax, 0");                                  // did libc lseek() succeed with a non-negative resulting file offset?
            emitter.instruction(&format!("jge {}", success_label));             // continue only when fseek() succeeded
            abi::emit_pop_reg(emitter, "r10");                                  // discard preserved fd on the fseek() failure path
            emitter.instruction("mov rax, -1");                                 // fseek() returns -1 on failure
            emitter.instruction(&format!("jmp {}", done_label));                // skip EOF reset after a failed seek
            emitter.label(&success_label);
            abi::emit_pop_reg(emitter, "r10");                                  // restore fd for EOF-flag reset after a successful seek
            emitter.instruction("lea r11, [rip + _eof_flags]");                 // materialize the eof-flag table for fseek()
            emitter.instruction("mov BYTE PTR [r11 + r10], 0");                 // clear EOF because fseek() repositioned the stream
            emitter.instruction("xor eax, eax");                                // fseek() returns 0 on success
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Int)
}
