//! Purpose:
//! Emits PHP `gethostbyname` calls.
//! Resolves a host name to its IPv4 address string.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Evaluates the host-name argument into the string result registers and
//!   delegates to `__rt_gethostbyname`, which resolves and renders the address
//!   or returns the host name unchanged when resolution fails.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `gethostbyname()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gethostbyname()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_gethostbyname");
    Some(PhpType::Str)
}
