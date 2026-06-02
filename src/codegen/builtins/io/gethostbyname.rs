//! Purpose:
//! Emits PHP `gethostbyname` calls.
//! Resolves a host name to its IPv4 address string.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Evaluates the host-name argument into the string result registers and
//!   delegates to `__rt_gethostbyname`, which resolves and renders the address
//!   or returns the host name unchanged when resolution fails.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

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
