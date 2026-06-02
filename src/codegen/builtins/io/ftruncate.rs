//! Purpose:
//! Emits PHP `ftruncate` builtin calls that resize an open file handle.
//! Validates the stream argument and forwards to the libc `ftruncate` runtime
//! helper, or to the userspace wrapper's `stream_truncate()` for synthetic fds.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Truncation length is materialized into the second integer argument register
//!   following the platform ABI before the runtime call.
//! - A descriptor `>= USER_WRAPPER_FD_BASE` (0x40000000) is a userspace wrapper
//!   handle, so the call is routed to `__rt_user_wrapper_ftruncate` instead.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for the PHP `ftruncate(stream, size)` builtin.
///
/// Validates the stream argument and extracts its file descriptor into the primary
/// integer register. Evaluates the size expression and moves it into the second
/// integer argument register per the platform ABI. A normal fd calls the libc
/// `__rt_ftruncate`; a synthetic userspace-wrapper fd (`>= 0x40000000`) is routed
/// to `__rt_user_wrapper_ftruncate`, which invokes the wrapper's `stream_truncate`.
///
/// Returns `PhpType::Bool` unconditionally.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ftruncate()");
    emit_stream_fd_arg("ftruncate", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the size expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the truncation length into the second runtime argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the primary integer register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the truncation length into the second runtime argument register
            abi::emit_pop_reg(emitter, "rax");                                  // restore the file descriptor into the primary integer register
        }
    }
    // -- user-wrapper synthetic fd path (G1): dispatch into stream_truncate --
    //    A descriptor >= USER_WRAPPER_FD_BASE is a userspace wrapper handle, so
    //    ftruncate() must call the wrapper's stream_truncate() rather than the
    //    libc ftruncate() syscall (which would fail on the synthetic fd).
    let wrapper_label = ctx.next_label("ftruncate_user_wrapper");
    let done_label = ctx.next_label("ftruncate_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // dispatch into the wrapper's stream_truncate
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // load USER_WRAPPER_FD_BASE for the synthetic-fd comparison
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // dispatch into the wrapper's stream_truncate
        }
    }
    abi::emit_call_label(emitter, "__rt_ftruncate");                            // call the libc ftruncate(fd, size) wrapper on a normal fd
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", done_label)),     // skip the wrapper path on the normal-fd result
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", done_label)),    // skip the wrapper path on the normal-fd result
    }
    emitter.label(&wrapper_label);
    // `__rt_user_wrapper_ftruncate` resolves the wrapper object from the
    // synthetic fd and calls stream_truncate($new_size). Its lookup expects the
    // fd in the SysV first-arg register (x0 / rdi) and the size in the second
    // (x1 / rsi). ARM64 already holds fd in x0 and size in x1; x86_64 left fd in
    // rax (the size is already in rsi), so move the fd into rdi first.
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                   // synthetic fd → wrapper-lookup first-arg register
    }
    abi::emit_call_label(emitter, "__rt_user_wrapper_ftruncate");               // call the wrapper's stream_truncate($new_size)
    emitter.label(&done_label);
    Some(PhpType::Bool)
}
