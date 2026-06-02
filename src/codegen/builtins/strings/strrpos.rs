//! Purpose:
//! Emits PHP `strrpos` string search or comparison calls.
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

/// Emits code for PHP `strrpos(haystack, needle)`.
///
/// Pushes the haystack pointer/length registers, evaluates the needle argument,
/// loads both strings into the ABI string-helper registers, calls `__rt_strrpos`,
/// then boxes the raw integer result (position or sentinel) into a `PhpType::Mixed`
/// value so PHP's `false | int` return type is preserved correctly.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strrpos()");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the haystack pointer and length while evaluating the needle string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the needle pointer into the third string-helper argument register
            emitter.instruction("mov x4, x2");                                  // move the needle length into the fourth string-helper argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the haystack pointer and length after evaluating the needle
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the haystack pointer and length while evaluating the needle string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the needle length into the fourth SysV string-helper argument register
            emitter.instruction("mov rdx, rax");                                // move the needle pointer into the third SysV string-helper argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the haystack pointer and length into the first two SysV helper argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_strrpos");                              // find the last needle occurrence in the haystack through the shared runtime helper
    box_search_result(emitter, ctx);

    Some(PhpType::Mixed)
}

/// Box the raw search result in `x0`/`rax` into a `PhpType::Mixed` value.
///
/// - If the result is negative (sentinel), emits `bool false` (tag 3).
/// - If the result is non-negative (found position), emits `int` (tag 0).
///
/// The distinction matters because PHP's `strrpos` returns `int 0` for a match
/// at position zero but `false` when nothing is found — both fit in a raw
/// integer register but have different PHP runtime representations.
fn box_search_result(emitter: &mut Emitter, ctx: &mut Context) {
    let found_label = ctx.next_label("strrpos_found");
    let end_label = ctx.next_label("strrpos_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // distinguish a valid non-negative match offset from the not-found sentinel
            emitter.instruction(&format!("b.ge {}", found_label));              // box a found offset as an integer result
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for the mixed bool box
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false for strrpos() not found
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so offset 0 remains distinguishable from not found
            emitter.instruction(&format!("b {}", end_label));                   // skip the integer boxing path after the not-found result
            emitter.label(&found_label);
            emitter.instruction("mov x1, x0");                                  // move the found offset into the mixed helper payload register
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads do not use a high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int for strrpos() found offsets
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the found integer offset as mixed
            emitter.label(&end_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // distinguish a valid non-negative match offset from the not-found sentinel
            emitter.instruction(&format!("jge {}", found_label));               // box a found offset as an integer result
            emitter.instruction("xor edi, edi");                                // false payload = 0 for the mixed bool box
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false for strrpos() not found
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so offset 0 remains distinguishable from not found
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the integer boxing path after the not-found result
            emitter.label(&found_label);
            emitter.instruction("mov rdi, rax");                                // move the found offset into the mixed helper payload register
            emitter.instruction("xor esi, esi");                                // integer mixed payloads do not use a high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int for strrpos() found offsets
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the found integer offset as mixed
            emitter.label(&end_label);
        }
    }
}
