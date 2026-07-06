//! Purpose:
//! Emits PHP `stream_socket_server` calls.
//! Opens a listening TCP socket and yields it as a PHP stream resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_stream_socket_server` helper returns the listening descriptor or
//!   -1; -1 is boxed as PHP false, a valid descriptor as a stream resource.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_socket_server()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_server()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // address pointer becomes the first helper argument
            emitter.instruction("mov x1, x2");                                  // address length becomes the second helper argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // address pointer becomes the first SysV argument
            emitter.instruction("mov rsi, rdx");                                // address length becomes the second SysV argument
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_server");
    box_socket_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a -1 descriptor becomes PHP `false`, any other
/// value becomes a stream resource. Shared with `stream_socket_client`.
pub(super) fn box_socket_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("sockserver_false");
    let done_label = ctx.next_label("sockserver_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the helper report a failed socket?
            emitter.instruction(&format!("b.lt {}", false_label));              // box PHP false on a -1 sentinel
            emitter.instruction("mov x1, x0");                                  // move the descriptor into the mixed payload
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads have no high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after success
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the helper report a failed socket?
            emitter.instruction(&format!("js {}", false_label));                // box PHP false on a -1 sentinel
            emitter.instruction("mov rdi, rax");                                // move the descriptor into the mixed payload
            emitter.instruction("xor esi, esi");                                // resource mixed payloads have no high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after success
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
