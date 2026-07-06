//! Purpose:
//! Emits PHP `stream_wrapper_restore` calls.
//! v1 always reports success — elephc's built-in wrappers cannot be
//! unregistered, so a restore is effectively a no-op.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The protocol argument is evaluated for its side effects and discarded;
//!   the result is `true`.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_wrapper_restore()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_wrapper_restore()");
    // Evaluate the protocol string for its side effects; the v1 stub always
    // reports success because built-in wrappers cannot be unregistered.
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #1"),                     // return true (built-in wrappers are always present)
        Arch::X86_64 => emitter.instruction("mov eax, 1"),                      // return true (built-in wrappers are always present)
    }
    Some(PhpType::Bool)
}
