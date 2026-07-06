//! Purpose:
//! Emits PHP `copy` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the PHP `copy($source, $dest)` builtin call.
///
/// Evaluates the source path expression (`args[0]`) and destination path expression
/// (`args[1]`), preserving PHP evaluation order (source first, destination second).
/// Saves the source string pointer/length pair across the destination evaluation,
/// then loads both into the appropriate ABI string slots before calling `__rt_copy`.
///
/// # Arguments
/// - `args[0]`: source path (string)
/// - `args[1]`: destination path (string)
///
/// # Returns
/// `PhpType::Bool` — PHP `copy()` returns `false` on failure, `true` on success.
///
/// # ABI Details
/// - AArch64: source pointer/length in `x1`/`x2`, dest pointer/length moved to `x3`/`x4`
/// - X86_64: source pointer/length in `rax`/`rdx`, dest pointer/length moved to `rdi`/`rsi`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("copy()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save the source path pointer and length while the destination expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the destination path pointer into the third string-argument slot
            emitter.instruction("mov x4, x2");                                  // move the destination path length into the fourth string-argument slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the source path pointer and length after evaluating the destination expression
            abi::emit_call_label(emitter, "__rt_copy");                         // call the target-aware runtime helper that copies the file-system path
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the source path pointer and length while the destination expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the destination path pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the destination path length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the source path pointer and length after evaluating the destination expression
            abi::emit_call_label(emitter, "__rt_copy");                         // call the target-aware runtime helper that copies the file-system path
        }
    }
    Some(PhpType::Bool)
}
