//! Purpose:
//! Emits PHP `stream_socket_client` calls.
//! Opens a connected TCP socket and yields it as a PHP stream resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_stream_socket_client` helper returns the connected descriptor or
//!   -1; the result is boxed by the shared `box_socket_result` helper.

use crate::codegen_support::builtins::io::stream_socket_server::box_socket_result;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_socket_client()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_client()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            // Preserve the address (ptr/len) across the connect so the connected
            // fd can be paired with its transport host for TLS SNI defaulting.
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] addr ptr, [sp,#8] addr len
            emitter.instruction("str x1, [sp, #0]");                            // save the address pointer
            emitter.instruction("str x2, [sp, #8]");                            // save the address length
            emitter.instruction("mov x0, x1");                                  // address pointer becomes the first helper argument
            emitter.instruction("mov x1, x2");                                  // address length becomes the second helper argument
            abi::emit_call_label(emitter, "__rt_stream_socket_client");
            // -- stash the transport host for this fd (passthrough: fd in x0 out x0) --
            emitter.instruction("ldr x1, [sp, #0]");                            // reload the address pointer
            emitter.instruction("ldr x2, [sp, #8]");                            // reload the address length
            emitter.instruction("add sp, sp, #16");                             // release the address scratch frame
            abi::emit_call_label(emitter, "__rt_stash_connect_host");           // record _stream_connect_host[fd], returns fd in x0
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0] addr ptr, [rsp+8] addr len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the address pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the address length
            emitter.instruction("mov rdi, rax");                                // address pointer becomes the first SysV argument
            emitter.instruction("mov rsi, rdx");                                // address length becomes the second SysV argument
            abi::emit_call_label(emitter, "__rt_stream_socket_client");
            // -- stash the transport host for this fd (passthrough: fd in rdi out rax) --
            emitter.instruction("mov rdi, rax");                                // connected fd becomes the stash arg0
            emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                // reload the address pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // reload the address length
            emitter.instruction("add rsp, 16");                                 // release the address scratch frame
            abi::emit_call_label(emitter, "__rt_stash_connect_host");           // record _stream_connect_host[fd], returns fd in rax
        }
    }
    box_socket_result(emitter, ctx);
    Some(PhpType::Mixed)
}
