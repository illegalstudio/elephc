//! Purpose:
//! Emits PHP `rename` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `rename($from, $to)` filesystem function.
///
/// Evaluates the source path first, saves its pointer/length registers while evaluating
/// the destination path, then calls `__rt_rename` to perform the actual OS rename.
///
/// # Arguments
/// - `_name`: Unused; builtin dispatch is handled at the call site.
/// - `args`: Two expressions — the source path and destination path.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context (types, locals, class metadata).
/// - `data`: Data section for string literals and constants.
///
/// # Returns
/// Always returns `PhpType::Bool` — PHP's rename returns false on failure, true on success.
///
/// # Implementation notes
/// - String arguments use pointer/length pairs: `x1`/`x2` on AArch64, `rax`/`rdx` on x86_64.
/// - Registers are spilled to the stack to preserve source-path data across destination evaluation.
/// - Calls `__rt_rename` which returns 0 on success and -1 on failure; the boolean reflects this.
/// - Effectful: observable OS filesystem mutation with PHP-visible ordering.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rename()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save the source path pointer and length while the destination expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the destination path pointer into the third string-argument slot
            emitter.instruction("mov x4, x2");                                  // move the destination path length into the fourth string-argument slot
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the source path pointer and length after evaluating the destination expression
            abi::emit_call_label(emitter, "__rt_rename");                       // call the target-aware runtime helper that renames the file-system path
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the source path pointer and length while the destination expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the destination path pointer into the third x86_64 string-argument slot
            emitter.instruction("mov rsi, rdx");                                // move the destination path length into the fourth x86_64 string-argument slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the source path pointer and length after evaluating the destination expression
            abi::emit_call_label(emitter, "__rt_rename");                       // call the target-aware runtime helper that renames the file-system path
        }
    }
    Some(PhpType::Bool)
}
