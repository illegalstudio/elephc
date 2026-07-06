//! Purpose:
//! Emits compiler-extension `ptr_write_string` raw memory writes.
//! Copies borrowed PHP string bytes into caller-owned raw memory without adding a terminator.
//!
//! Called from:
//! - `crate::codegen_support::builtins::pointers::emit()`.
//!
//! Key details:
//! - The source string remains borrowed; the helper returns the number of payload bytes copied.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `ptr_write_string` builtin: copies PHP string bytes into raw memory.
/// Does not append a NUL terminator. Source string remains borrowed. Returns the
/// number of bytes copied as `PhpType::Int`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_write_string() — copy PHP string bytes into raw memory");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before writing to memory
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the destination pointer while the source string is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x0");                                   // restore the destination pointer while leaving the string pair in x1/x2
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the destination pointer into the x86_64 helper destination register
        }
    }
    abi::emit_call_label(emitter, "__rt_ptr_write_string");                     // copy the borrowed string payload and return its byte length
    Some(PhpType::Int)
}
