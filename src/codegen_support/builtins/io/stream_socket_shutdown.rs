//! Purpose:
//! Emits PHP `stream_socket_shutdown` calls.
//! Disables further reception and/or transmission on a socket resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Unboxes the socket resource to its descriptor and evaluates the `how`
//!   mode, then delegates to `__rt_stream_socket_shutdown`, which returns bool.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `stream_socket_shutdown()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_shutdown()");
    emit_stream_fd_arg("stream_socket_shutdown", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // shutdown mode into the second helper argument
            abi::emit_pop_reg(emitter, "x0"); // descriptor into the first helper argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // shutdown mode into the second SysV argument
            abi::emit_pop_reg(emitter, "rdi"); // descriptor into the first SysV argument
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_shutdown");
    Some(PhpType::Bool)
}
