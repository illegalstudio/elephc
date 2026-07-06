//! Purpose:
//! Emits PHP `pclose` calls.
//! Closes a process pipe opened by `popen()` and yields the child status.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The descriptor is unboxed from the stream resource and handed to the
//!   `__rt_pclose` runtime helper, which calls libc `pclose`.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `pclose()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pclose()");
    emit_stream_fd_arg("pclose", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the descriptor into the runtime-helper argument register
    }
    abi::emit_call_label(emitter, "__rt_pclose");
    Some(PhpType::Int)
}
