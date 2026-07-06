//! Purpose:
//! Emits PHP `tempnam` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `tempnam(dir, prefix)` builtin call.
///
/// Evaluates `dir` (args[0]) first, then `prefix` (args[1]), marshaling both as
/// string pairs into the runtime helper `__rt_tempnam`. On ARM64 the directory
/// pair is saved/restored via the stack around the prefix evaluation; on x86_64
/// it is preserved in `rax`/`rdx`. Returns `PhpType::Str` on success,
/// or the caller handles PHP false-on-failure semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("tempnam()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push dir ptr and length onto the stack while the prefix expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the prefix pointer into the third ARM64 string-argument slot
            emitter.instruction("mov x4, x2");                                  // move the prefix length into the fourth ARM64 string-argument slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the directory string pair after evaluating the prefix expression
            abi::emit_call_label(emitter, "__rt_tempnam");                      // call the target-aware runtime helper that builds the temp filename
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the directory string pair while the prefix expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the prefix pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the prefix length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the directory string pair after evaluating the prefix expression
            abi::emit_call_label(emitter, "__rt_tempnam");                      // call the target-aware runtime helper that builds the temp filename
        }
    }
    Some(PhpType::Str)
}
