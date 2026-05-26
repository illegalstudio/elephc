//! Purpose:
//! Emits PHP `link` (hard link) builtin calls.
//! Marshals old / new path arguments and invokes the libc wrapper runtime.
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

/// Lowers PHP `link(oldpath, newpath)` into a `__rt_link` libc call.
///
/// Marshals two string arguments (old path, new path) according to target ABI.
/// On both ARM64 and x86_64 the evaluation order preserves source semantics:
/// the old path is evaluated and preserved on the stack or in callee-saved registers
/// before the new path is evaluated, then both are loaded into argument registers
/// in the order `link(oldpath, newpath)` expects before the runtime wrapper is called.
///
/// # Arguments
/// - `_name`: Unused — the builtin name is not needed at emission time.
/// - `args`: Exactly two expressions: `args[0]` = old path, `args[1]` = new path.
/// - `emitter`: Assembly emitter.
/// - `ctx`: Codegen context (used by `emit_expr`).
/// - `data`: Data section for string literals.
///
/// # Return
/// Always returns `PhpType::Bool` (the call result is a libc `int` converted to bool).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("link()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve old path while new path is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move new path pointer
            emitter.instruction("mov x4, x2");                                  // move new path length
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore old path
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve old path
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // new path pointer
            emitter.instruction("mov rsi, rdx");                                // new path length
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore old path
        }
    }
    abi::emit_call_label(emitter, "__rt_link");                                 // libc link(oldpath, newpath) wrapper
    Some(PhpType::Bool)
}
