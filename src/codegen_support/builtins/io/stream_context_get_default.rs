//! Purpose:
//! Emits PHP `stream_context_get_default` calls.
//! v1 returns a placeholder context resource matching `stream_context_create`
//! so PHP code that consults the default context compiles cleanly.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The optional options argument is evaluated for its side effects. Options
//!   storage on the returned resource is deferred along with the rest of the
//!   `stream_context_*` family.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_get_default()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_get_default()");
    // Evaluate the optional options for its side effects; v1 does not yet
    // persist them on the returned default context.
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #0"),                     // placeholder default-context resource identifier
        Arch::X86_64 => emitter.instruction("xor eax, eax"),                    // placeholder default-context resource identifier
    }
    Some(PhpType::stream_resource())
}
