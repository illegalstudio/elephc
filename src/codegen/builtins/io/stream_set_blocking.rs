//! Purpose:
//! Emits PHP `stream_set_blocking` calls.
//! Toggles a stream's blocking mode through the runtime fcntl helper, or — for a
//! synthetic userspace-wrapper descriptor — through the wrapper's
//! `stream_set_option(STREAM_OPTION_BLOCKING, $mode, 0)`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Unboxes the stream resource to its descriptor, evaluates the blocking
//!   flag, and delegates to `__rt_stream_set_blocking` (fcntl) for a normal fd.
//! - A descriptor `>= USER_WRAPPER_FD_BASE` (0x40000000) is a userspace wrapper
//!   handle, so the call is routed to `__rt_user_wrapper_set_option` (vtable slot
//!   13) with option `STREAM_OPTION_BLOCKING` and the blocking flag as `$arg1`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// PHP `STREAM_OPTION_BLOCKING` option value passed to `stream_set_option`.
const STREAM_OPTION_BLOCKING: usize = 1;

/// Emits the `stream_set_blocking(resource $stream, bool $enable)` builtin.
///
/// Materializes the descriptor (x0 / rdi) and the blocking flag (x1 / rsi), then
/// dispatches: a synthetic wrapper fd (`>= 0x40000000`) calls
/// `__rt_user_wrapper_set_option(fd, STREAM_OPTION_BLOCKING, flag, 0)`; any other
/// fd calls the libc `__rt_stream_set_blocking(fd, flag)`. Returns `PhpType::Bool`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_set_blocking()");
    emit_stream_fd_arg("stream_set_blocking", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    let wrapper = ctx.next_label("set_blocking_wrapper");
    let after = ctx.next_label("set_blocking_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // blocking flag into the second helper argument
            abi::emit_pop_reg(emitter, "x0");                                   // descriptor into the first helper argument
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper));                  // dispatch into the wrapper's stream_set_option
            abi::emit_call_label(emitter, "__rt_stream_set_blocking");         // normal fd: fcntl O_NONBLOCK toggle
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov x2, x1");                                  // arg1 = blocking flag
            emitter.instruction(&format!("mov x1, #{}", STREAM_OPTION_BLOCKING)); // option = STREAM_OPTION_BLOCKING
            emitter.instruction("mov x3, #0");                                  // arg2 = 0 (unused for blocking)
            abi::emit_call_label(emitter, "__rt_user_wrapper_set_option");     // call the wrapper's stream_set_option($option, $arg1, $arg2)
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // blocking flag into the second SysV argument
            abi::emit_pop_reg(emitter, "rdi");                                  // descriptor into the first SysV argument
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rdi, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper));                   // dispatch into the wrapper's stream_set_option
            abi::emit_call_label(emitter, "__rt_stream_set_blocking");         // normal fd: fcntl O_NONBLOCK toggle
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdx, rsi");                                // arg1 = blocking flag
            emitter.instruction(&format!("mov rsi, {}", STREAM_OPTION_BLOCKING)); // option = STREAM_OPTION_BLOCKING
            emitter.instruction("xor ecx, ecx");                                // arg2 = 0 (unused for blocking)
            abi::emit_call_label(emitter, "__rt_user_wrapper_set_option");     // call the wrapper's stream_set_option($option, $arg1, $arg2)
            emitter.label(&after);
        }
    }
    Some(PhpType::Bool)
}
