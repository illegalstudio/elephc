//! Purpose:
//! Emits PHP `str_contains` string search or comparison calls.
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

/// Emits the `str_contains(haystack, needle)` builtin call.
///
/// Saves the haystack pointer/length before evaluating the needle, then calls
/// the shared `__rt_strpos` runtime helper and normalizes the signed position result
/// to a PHP boolean (true if needle is found at any position including 0).
///
/// # Arguments
/// - `args[0]`: haystack string expression
/// - `args[1]`: needle string expression
///
/// # Returns
/// `PhpType::Bool` — always returns a boolean regardless of whether the needle was
/// found at position 0 or not found at all, distinguishing PHP false from integer 0.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_contains()");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto the stack while evaluating the needle string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the needle pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the needle length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after evaluating the needle
            abi::emit_call_label(emitter, "__rt_strpos");                       // search the haystack for the needle through the shared runtime helper
            emitter.instruction("cmp x0, #0");                                  // check whether strpos() returned a non-negative match position
            emitter.instruction("cset x0, ge");                                 // normalize the signed strpos() result into a PHP boolean integer
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the haystack pointer and length while evaluating the needle string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the needle length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the needle pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the haystack pointer and length into the first two SysV helper argument registers
            abi::emit_call_label(emitter, "__rt_strpos");                       // search the haystack for the needle through the shared runtime helper
            emitter.instruction("cmp rax, 0");                                  // check whether strpos() returned a non-negative match position
            emitter.instruction("setge al");                                    // normalize the signed strpos() result into the low boolean byte
            emitter.instruction("movzx eax, al");                               // widen the normalized boolean byte into the integer result register
        }
    }

    Some(PhpType::Bool)
}
