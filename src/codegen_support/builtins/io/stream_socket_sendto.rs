//! Purpose:
//! Emits PHP `stream_socket_sendto` calls.
//! Sends a message on a socket, optionally to an explicit datagram address.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals the descriptor, data string, optional flags, and optional
//!   address string into the six `__rt_stream_socket_sendto` argument
//!   registers; omitted flags default to 0 and an omitted address to empty.
//! - The helper returns the byte count, or -1 boxed as PHP false.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `stream_socket_sendto()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_sendto()");
    emit_stream_fd_arg("stream_socket_sendto", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x1"); // preserve the data pointer
            abi::emit_push_reg(emitter, "x2"); // preserve the data length
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax"); // preserve the data pointer
            abi::emit_push_reg(emitter, "rdx"); // preserve the data length
        }
    }
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #0"),                 // omitted flags default to 0
            Arch::X86_64 => emitter.instruction("xor eax, eax"),                // omitted flags default to 0
        }
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the send flags
    match emitter.target.arch {
        Arch::AArch64 => {
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov x4, x1");                              // address pointer into argument 4
                emitter.instruction("mov x5, x2");                              // address length into argument 5
            } else {
                emitter.instruction("mov x4, #0");                              // omitted address: NULL pointer
                emitter.instruction("mov x5, #0");                              // omitted address: zero length
            }
            abi::emit_pop_reg(emitter, "x3"); // send flags into argument 3
            abi::emit_pop_reg(emitter, "x2"); // data length into argument 2
            abi::emit_pop_reg(emitter, "x1"); // data pointer into argument 1
            abi::emit_pop_reg(emitter, "x0"); // descriptor into argument 0
        }
        Arch::X86_64 => {
            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("mov r8, rax");                             // address pointer into argument 5
                emitter.instruction("mov r9, rdx");                             // address length into argument 6
            } else {
                emitter.instruction("xor r8d, r8d");                            // omitted address: NULL pointer
                emitter.instruction("xor r9d, r9d");                            // omitted address: zero length
            }
            abi::emit_pop_reg(emitter, "rcx"); // send flags into argument 4
            abi::emit_pop_reg(emitter, "rdx"); // data length into argument 3
            abi::emit_pop_reg(emitter, "rsi"); // data pointer into argument 2
            abi::emit_pop_reg(emitter, "rdi"); // descriptor into argument 1
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_sendto");
    box_count_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a -1 sentinel becomes PHP `false`, any other value
/// becomes a boxed integer byte count.
fn box_count_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("stream_socket_sendto_false");
    let done_label = ctx.next_label("stream_socket_sendto_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the helper report a failed send?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false on the -1 sentinel
            emitter.instruction("mov x1, x0");                                  // move the byte count into the mixed payload
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads have no high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid send
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the helper report a failed send?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false on the -1 sentinel
            emitter.instruction("mov rdi, rax");                                // move the byte count into the mixed payload
            emitter.instruction("xor esi, esi");                                // integer mixed payloads have no high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = integer
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid send
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
