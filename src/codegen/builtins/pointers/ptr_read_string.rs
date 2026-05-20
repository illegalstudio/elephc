//! Purpose:
//! Emits compiler-extension `ptr_read_string` raw memory reads.
//! Bridges a checked raw pointer plus byte length into an owned elephc PHP string result.
//!
//! Called from:
//! - `crate::codegen::builtins::pointers::emit()`.
//!
//! Key details:
//! - The runtime helper copies exactly the requested byte count and does not scan for NUL.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_read_string() — copy raw bytes into an owned PHP string");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before reading from memory
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the source pointer while the length expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the requested byte length into the runtime helper length register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the validated source pointer for the runtime helper
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdx, rax");                                // move the requested byte length into the x86_64 string-length register
            abi::emit_pop_reg(emitter, "rax");                                  // restore the validated source pointer for the runtime helper
        }
    }
    abi::emit_call_label(emitter, "__rt_ptr_read_string");                      // allocate and copy the exact raw byte slice into an owned PHP string
    Some(PhpType::Str)
}
