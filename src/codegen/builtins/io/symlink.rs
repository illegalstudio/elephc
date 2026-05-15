//! Purpose:
//! Emits PHP `symlink` builtin calls.
//! Marshals target / link path arguments and invokes the libc wrapper runtime.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returns `true` on success, `false` on failure.

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
    emitter.comment("symlink()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve target ptr/len while link is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move link pointer into the secondary string-argument pair
            emitter.instruction("mov x4, x2");                                  // move link length into the secondary string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore target ptr/len
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve target ptr/len
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // link → secondary string pointer
            emitter.instruction("mov rsi, rdx");                                // link → secondary string length
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore target ptr/len
        }
    }
    abi::emit_call_label(emitter, "__rt_symlink");                              // libc symlink(target, link) wrapper
    Some(PhpType::Bool)
}
