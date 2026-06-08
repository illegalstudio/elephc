//! Purpose:
//! Emits PHP `strcasecmp` string search or comparison calls.
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
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `strcasecmp` builtin.
///
/// Compares two strings case-insensitively via the `__rt_strcasecmp` runtime helper.
/// Stores both string pointers/lengths on the stack (ARM64) or in registers (X86_64) to
/// preserve evaluation order before the call.
///
/// # Arguments
/// * `_name` - Unused; present for dispatcher uniformity.
/// * `args` - Two expressions: the first and second strings to compare.
/// * `emitter` - Target-specific assembly emitter.
/// * `ctx` - Codegen context (used for expression lowering).
/// * `data` - Data section for string literals and constants.
///
/// # Returns
/// Always returns `Some(PhpType::Int)`. PHP's `strcasecmp` returns integer 0 when strings
/// are equal, a negative value if `s1` is less than `s2`, or a positive value otherwise.
///
/// # ABI Details
/// - ARM64: first string pointer/length in x1/x2, second in x3/x4; order preserved via stack push/pop.
/// - X86_64: first string pointer/length in rdi/rsi, second in rdx/rcx; order preserved via register stack.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strcasecmp()");
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the first string pointer and length while evaluating the second string
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the second string pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the second string length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the first string pointer and length after evaluating the second string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the first string pointer and length while evaluating the second string
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the second string length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the second string pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the first string pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // compare both strings case-insensitively through the shared runtime helper

    Some(PhpType::Int)
}
