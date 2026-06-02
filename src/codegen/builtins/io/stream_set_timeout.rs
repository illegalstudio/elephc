//! Purpose:
//! Emits PHP `stream_set_timeout()` calls.
//! Sets a stream's read timeout through the runtime helper, or — for a synthetic
//! userspace-wrapper descriptor — through the wrapper's
//! `stream_set_option(STREAM_OPTION_READ_TIMEOUT, $seconds, $microseconds)`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals the descriptor, seconds, and optional microseconds, and calls
//!   `__rt_stream_set_timeout` (setsockopt SO_RCVTIMEO) for a normal fd. The
//!   microseconds argument defaults to 0 when omitted.
//! - A descriptor `>= USER_WRAPPER_FD_BASE` (0x40000000) is a userspace wrapper
//!   handle, so the call is routed to `__rt_user_wrapper_set_option` (vtable slot
//!   13) with option `STREAM_OPTION_READ_TIMEOUT`, the seconds as `$arg1`, and
//!   the microseconds as `$arg2`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// PHP `STREAM_OPTION_READ_TIMEOUT` option value passed to `stream_set_option`.
const STREAM_OPTION_READ_TIMEOUT: usize = 4;

/// Emits the `stream_set_timeout(resource $stream, int $seconds, int $usec = 0)`
/// builtin.
///
/// Materializes the descriptor, seconds, and (optional) microseconds, then
/// dispatches: a synthetic wrapper fd (`>= 0x40000000`) calls
/// `__rt_user_wrapper_set_option(fd, STREAM_OPTION_READ_TIMEOUT, seconds, usec)`;
/// any other fd calls the libc `__rt_stream_set_timeout(fd, seconds, usec)`.
/// Returns `PhpType::Bool`.
///
/// Register layout at the dispatch point (after the args are materialized):
/// AArch64 fd=x0, seconds=x1, usec=x2; x86_64 fd=rdi, seconds=rsi, usec=rdx.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_set_timeout()");
    emit_stream_fd_arg("stream_set_timeout", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the seconds value
    // -- materialize fd / seconds / usec into the libc-call registers --
    match emitter.target.arch {
        Arch::AArch64 => {
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x2, x0");                              // microseconds into argument 2
            } else {
                emitter.instruction("mov x2, #0");                              // no microseconds argument: default to 0
            }
            abi::emit_pop_reg(emitter, "x1");                                   // seconds into argument 1
            abi::emit_pop_reg(emitter, "x0");                                   // descriptor into argument 0
        }
        Arch::X86_64 => {
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov rdx, rax");                            // microseconds into argument 2
            } else {
                emitter.instruction("xor edx, edx");                            // no microseconds argument: default to 0
            }
            abi::emit_pop_reg(emitter, "rsi");                                  // seconds into argument 1
            abi::emit_pop_reg(emitter, "rdi");                                  // descriptor into argument 0
        }
    }
    // -- dispatch: synthetic wrapper fd → stream_set_option, else libc setsockopt --
    let wrapper = ctx.next_label("set_timeout_wrapper");
    let after = ctx.next_label("set_timeout_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper));                  // dispatch into the wrapper's stream_set_option
            abi::emit_call_label(emitter, "__rt_stream_set_timeout");          // normal fd: setsockopt SO_RCVTIMEO(fd, seconds, usec)
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            // remap libc (fd=x0, sec=x1, usec=x2) → set_option(fd, option, arg1=sec, arg2=usec)
            emitter.instruction("mov x3, x2");                                  // arg2 = microseconds
            emitter.instruction("mov x2, x1");                                  // arg1 = seconds
            emitter.instruction(&format!("mov x1, #{}", STREAM_OPTION_READ_TIMEOUT)); // option = STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(emitter, "__rt_user_wrapper_set_option");     // call the wrapper's stream_set_option($option, $arg1, $arg2)
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rdi, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper));                   // dispatch into the wrapper's stream_set_option
            abi::emit_call_label(emitter, "__rt_stream_set_timeout");          // normal fd: setsockopt SO_RCVTIMEO(fd, seconds, usec)
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            // remap libc (fd=rdi, sec=rsi, usec=rdx) → set_option(fd, option, arg1=sec, arg2=usec)
            emitter.instruction("mov rcx, rdx");                                // arg2 = microseconds
            emitter.instruction("mov rdx, rsi");                                // arg1 = seconds
            emitter.instruction(&format!("mov rsi, {}", STREAM_OPTION_READ_TIMEOUT)); // option = STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(emitter, "__rt_user_wrapper_set_option");     // call the wrapper's stream_set_option($option, $arg1, $arg2)
            emitter.label(&after);
        }
    }
    Some(PhpType::Bool)
}
