//! Purpose:
//! Emits PHP `str_starts_with` string search or comparison calls.
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

/// Emits the `str_starts_with(haystack, prefix)` builtin call.
///
/// Evaluates `haystack` (args[0]) and `prefix` (args[1]) as strings, then calls the
/// runtime helper `__rt_str_starts_with` to check whether the haystack begins with the prefix.
///
/// Arguments:
/// - `args[0]`: haystack string expression
/// - `args[1]`: prefix string expression
/// - `emitter`: target-aware assembly emitter; saves haystack registers, evaluates prefix, restores haystack, calls runtime
/// - `ctx`: codegen context for variable layout and ownership
/// - `data`: data section for relocations and string literals
///
/// Returns `Some(PhpType::Bool)` — `str_starts_with` always produces a boolean.
///
/// ABI Details:
/// - AArch64: pushes haystack ptr/length (x1/x2) on stack while evaluating prefix (x3/x4), then restores haystack before the call
/// - x86_64: pushes haystack ptr/length (rax/rdx) on stack while evaluating prefix (rcx/rdx), then pops into rdi/rsi before the call
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_starts_with()");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the haystack pointer and length while evaluating the prefix string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the prefix pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the prefix length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after evaluating the prefix
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the haystack pointer and length while evaluating the prefix string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the prefix length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the prefix pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the haystack pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_starts_with");                      // check whether the haystack begins with the provided prefix through the target-aware runtime helper

    Some(PhpType::Bool)
}
