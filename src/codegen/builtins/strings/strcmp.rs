//! Purpose:
//! Emits PHP `strcmp` string search or comparison calls.
//! Handles string pointer/length arguments and boxes false-or-position results when PHP requires mixed output.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Return values must distinguish numeric position zero from PHP false.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for a PHP `strcmp(left, right)` call.
///
/// Compares two strings lexicographically via the `__rt_strcmp` runtime helper.
/// Evaluates both argument expressions (which must resolve to strings), materializes
/// them as pointer/length pairs in the appropriate ABI registers, and calls the runtime
/// routine. The result is always `PhpType::Int` (0 for equal, <0 or >0 for ordering).
///
/// # Arguments
/// - `args` — exactly two expressions: the left and right strings to compare.
/// - `emitter` — target-specific instruction emission.
/// - `ctx` — codegen context carrying variable layout and metadata.
/// - `data` — data section for relocations and static strings.
///
/// # ABI details
/// - AArch64: first string in x1/x2, second string in x3/x4 via temporary stack spill.
/// - x86_64: first string in rdi/rsi, second string in rdx/rcx via temporary stack spill.
/// - Both targets call `__rt_strcmp` and the integer result is returned in the usual
///   integer register (`x0` on AArch64, `rax` on x86_64).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strcmp()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the first string pointer and length while evaluating the second string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the second string pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the second string length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the first string pointer and length after evaluating the second string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the first string pointer and length while evaluating the second string
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the second string length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the second string pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the first string pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_strcmp");                               // compare both strings lexicographically through the shared runtime helper

    Some(PhpType::Int)
}
