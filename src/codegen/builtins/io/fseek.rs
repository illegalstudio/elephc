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

/// Emits the `fseek(stream, offset, whence)` builtin call.
///
/// Validates the stream resource and unboxes it to a raw file descriptor,
/// evaluates `offset` and optional `whence` arguments (defaulting to SEEK_SET=0),
/// then calls the platform lseek syscall. On success the stream's EOF flag is
/// cleared before returning 0. On failure returns -1.
///
/// # Arguments
/// - `_name`: builtin name (unused, always "fseek")
/// - `args`: [stream, offset, whence?] — whence is optional, defaults to 0 (SEEK_SET)
/// - `emitter`: target for emitted assembly
/// - `ctx`: codegen context (labels, target, platform)
/// - `data`: data section for symbols (eof_flags table)
///
/// # Returns
/// Always `Some(PhpType::Int)` — PHP semantics: 0 on success, -1 on failure.
///
/// # Side effects
/// - Clobbers caller-saved registers used for syscall argument passing.
/// - Stack: pushes two registers before evaluating offset/whence, pops on completion.
/// - On success: clears the per-fd EOF flag via the `_eof_flags` runtime symbol.
///
/// # ABI constraints
/// - AArch64: lseek via syscall 199, args in x0 (fd), x1 (offset), x2 (whence).
/// - x86_64: lseek via libc call, args in rdi (fd), rsi (offset), rdx (whence).
/// - Preserves fd on the stack across expression evaluation to handle errors safely.
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
    let user_wrapper_label = ctx.next_label("fseek_user_wrapper");
    let after_dispatch = ctx.next_label("fseek_after_dispatch");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x2, x0");                                  // move the whence selector into the third lseek syscall argument register
            abi::emit_pop_reg(emitter, "x1");                                   // restore the seek offset into the second lseek syscall argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the first lseek syscall argument register
            // -- user-wrapper synthetic fd path (Phase 10 step 4) --
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", user_wrapper_label));       // dispatch into the wrapper's stream_seek instead of lseek
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
            emitter.instruction(&format!("b {}", after_dispatch));              // skip the user-wrapper path on the normal-fd success/failure
            emitter.label(&user_wrapper_label);
            abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");           // dispatch into the wrapper's stream_seek
            emitter.label(&after_dispatch);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // move the whence selector into the third SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the seek offset into the second SysV lseek() argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV lseek() argument register
            // -- user-wrapper synthetic fd path (Phase 10 step 4) --
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rdi, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", user_wrapper_label));        // dispatch into the wrapper's stream_seek instead of lseek
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
            emitter.instruction(&format!("jmp {}", after_dispatch));            // skip the user-wrapper path on the normal-fd success/failure
            emitter.label(&user_wrapper_label);
            abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");           // dispatch into the wrapper's stream_seek
            emitter.label(&after_dispatch);
        }
    }
    Some(PhpType::Int)
}
