//! Purpose:
//! Emits PHP `stream_context_set_default($options)` calls. Returns the
//! default-context resource (matching `stream_context_get_default`).
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - v1 evaluates the options array for side effects but does not yet
//!   walk-and-apply each entry into `_stream_context_options`. PHP code
//!   that needs the options persisted should use repeated
//!   `stream_context_set_option(stream_context_get_default(), ...)`
//!   calls, which DO mutate the default context's options table.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_set_default()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_set_default()");
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #0"),                     // placeholder default-context resource identifier
        Arch::X86_64 => emitter.instruction("xor eax, eax"),                    // placeholder default-context resource identifier
    }
    Some(PhpType::stream_resource())
}
