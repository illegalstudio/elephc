//! Purpose:
//! Emits PHP `explode` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `explode(delimiter, string)` builtin.
/// Saves the delimiter (pointer in x1, length in x2), evaluates the subject string into
/// argument registers, restores the delimiter, then calls `__rt_explode` to split the string.
/// Returns an array of strings (`PhpType::Array(Str)`).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("explode()");
    // explode($delimiter, $string)
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- save delimiter, evaluate string --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the delimiter pointer and length while the subject-string expression is evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the subject-string pointer into the third AArch64 string-argument register
            emitter.instruction("mov x4, x2");                                  // move the subject-string length into the fourth AArch64 string-argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the delimiter pointer and length after evaluating the subject string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the delimiter pointer and length while the subject-string expression is evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the subject-string pointer into the third x86_64 string-argument register
            emitter.instruction("mov rsi, rdx");                                // move the subject-string length into the fourth x86_64 string-argument register
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the delimiter pointer and length after evaluating the subject string
        }
    }
    abi::emit_call_label(emitter, "__rt_explode");                              // split the subject string by the delimiter through the target-aware runtime helper

    Some(PhpType::Array(Box::new(PhpType::Str)))
}
