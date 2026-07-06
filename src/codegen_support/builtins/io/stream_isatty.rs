//! Purpose:
//! Emits PHP `stream_isatty` calls.
//! Resolves the stream resource to its descriptor and asks the runtime whether it is a terminal.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Delegates the `ioctl` terminal probe to the `__rt_stream_isatty` runtime helper.

use crate::codegen_support::abi;
use crate::codegen_support::builtins::io::stream_arg::emit_stream_fd_arg;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_isatty()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_isatty()");
    // Resolve the stream resource to its underlying file descriptor; the helper
    // validates the argument and leaves the descriptor in the result register.
    emit_stream_fd_arg("stream_isatty", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the descriptor into the runtime-helper argument register
    }
    abi::emit_call_label(emitter, "__rt_stream_isatty");
    Some(PhpType::Bool)
}
