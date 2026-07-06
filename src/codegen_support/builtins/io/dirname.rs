//! Purpose:
//! Emits PHP `dirname` path-oriented builtin calls.
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

/// Emits the `dirname(path)` or `dirname(path, levels)` builtin call.
///
/// For the single-argument form, calls `__rt_dirname` which strips the final path component.
/// For the two-argument form, calls `__rt_dirname_levels` which applies `dirname()` iteratively
/// `levels` times.
///
/// # Arguments
/// * `args[0]` - the path string (passed in ABI registers for the path pointer/length)
/// * `args[1]` - optional recursion depth (only present for 2-argument call)
///
/// # Output
/// * Returns `PhpType::Str` on success; the runtime helper returns null on failure which is
///   handled by the false-on-failure semantics in the caller.
///
/// # ABI details
/// * AArch64: path in `x1`/`x2`, levels in `x3`
/// * x86_64: path in `rax`/`rdx`, levels in `rdi`
/// * Preserves path registers across the levels expression evaluation.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("dirname()");
    emit_expr(&args[0], emitter, ctx, data);
    if args.len() == 1 {
        abi::emit_call_label(emitter, "__rt_dirname");                          // call the target-aware runtime helper that returns the parent-directory portion
        return Some(PhpType::Str);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the path ptr/len while the levels expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move the requested parent depth into the runtime levels register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the path ptr/len after evaluating the levels expression
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the path ptr/len while the levels expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the requested parent depth into the x86_64 runtime levels register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the path ptr/len after evaluating the levels expression
        }
    }
    abi::emit_call_label(emitter, "__rt_dirname_levels");                       // call the target-aware runtime helper that applies dirname() repeatedly
    Some(PhpType::Str)
}
