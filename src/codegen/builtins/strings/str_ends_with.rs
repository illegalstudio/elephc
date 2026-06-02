//! Purpose:
//! Emits PHP `str_ends_with` string search or comparison calls.
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
use super::args::emit_string_arg;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `str_ends_with` builtin call.
///
/// Arguments:
///   - `args[0]`: haystack string (AArch64: pointer in x1, length in x2; X86_64: pointer in rdi, length in rdx)
///   - `args[1]`: suffix string to search for at the haystack end
///
/// ABI behavior:
///   - AArch64: pushes haystack to stack, evaluates suffix into x3/x4, restores haystack from stack into x1/x2
///   - X86_64: saves haystack in rax/rdx, evaluates suffix into rcx/rdx, pops haystack into rdi/rsi
///   - Calls `__rt_str_ends_with` runtime helper that returns position or false
///
/// Returns: `PhpType::Bool` — PHP false when suffix is not found, otherwise a truthy position value
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_ends_with()");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the haystack pointer and length while evaluating the suffix string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the suffix pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the suffix length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after evaluating the suffix
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the haystack pointer and length while evaluating the suffix string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the suffix length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the suffix pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the haystack pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_ends_with");                        // check whether the haystack ends with the provided suffix through the target-aware runtime helper

    Some(PhpType::Bool)
}
