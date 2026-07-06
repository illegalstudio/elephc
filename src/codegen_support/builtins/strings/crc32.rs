//! Purpose:
//! Emits PHP `crc32()` calls: marshals the single string argument into the
//! `__rt_crc32` runtime helper, which returns the CRC-32 checksum as an int.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - `crc32($string): int` — the argument is evaluated (leaving the string
//!   ptr/len in the runtime string registers) and the helper returns the
//!   non-negative 32-bit checksum in the integer result register.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `crc32()` builtin, returning `PhpType::Int`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("crc32()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_crc32");                                // compute the CRC-32 of the input string → non-negative int in the result register
    Some(PhpType::Int)
}
