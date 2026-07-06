//! Purpose:
//! Emits PHP `stream_context_get_params` calls.
//! v1 stub: returns an empty associative array (allocated through
//! `__rt_hash_new`) because contexts do not yet persist their parameters.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Mirrors `stream_context_get_options`: the context is evaluated for its
//!   side effects and an empty Mixed-valued associative hash is returned.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_context_get_params()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_context_get_params()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // initial capacity (minimum non-zero)
            emitter.instruction("mov x1, #7");                                  // value type tag = Mixed
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 1");                                  // initial capacity (minimum non-zero)
            emitter.instruction("mov esi, 7");                                  // value type tag = Mixed
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_new");
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
