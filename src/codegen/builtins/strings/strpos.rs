//! Purpose:
//! Emits PHP `strpos` string search or comparison calls.
//! Handles string pointer/length arguments and boxes false-or-position results when PHP requires mixed output.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Return values must distinguish numeric position zero from PHP false.

use super::args::emit_string_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `strpos(haystack, needle)` builtin call.
///
/// Evaluate `haystack` first, then `needle`, arranging arguments in target-specific
/// string-helper registers before calling `__rt_strpos`. The runtime returns either a
/// non-negative byte offset (including 0 for a match at the start) or a sentinel to
/// indicate no match. Calls `box_search_result` to box the raw result as a `Mixed`
/// value so PHP can distinguish integer `0` from boolean `false`.
///
/// Returns `Some(PhpType::Mixed)` because `strpos` returns `int|false` in PHP.
///
/// # Arguments
/// * `_name` - Unused; the caller dispatches by name
/// * `args` - `[haystack, needle]`
/// * `emitter` - Target assembly emitter
/// * `ctx` - Codegen context for labels and target info
/// * `data` - Data section for relocations
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strpos()");
    // Coerce both operands to string (ptr/len) so a Mixed/Union haystack or
    // needle — e.g. stream_socket_get_name()'s `string|false` result — is
    // unboxed via __rt_mixed_cast_string rather than passed as a boxed cell.
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
    abi::emit_call_label(emitter, "__rt_strpos");                               // find the first needle occurrence in the haystack through the shared runtime helper
    box_search_result(emitter, ctx);

    Some(PhpType::Mixed)
}

/// Box a raw `strpos` result as a `Mixed` value.
///
/// Reads the raw integer result from `x0` (ARM64) or `rax` (x86_64). If the value
/// is non-negative, it is boxed as an integer (`tag = 0`). Otherwise, the not-found
/// sentinel is boxed as boolean `false` (`tag = 3`), preserving PHP's requirement that
/// `strpos(...) === false` and `strpos(...) !== 0` are both meaningful.
///
/// Uses `ctx.next_label` to generate local branch labels unique to this invocation.
fn box_search_result(emitter: &mut Emitter, ctx: &mut Context) {
    let found_label = ctx.next_label("strpos_found");
    let end_label = ctx.next_label("strpos_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // distinguish a valid non-negative match offset from the not-found sentinel
            emitter.instruction(&format!("b.ge {}", found_label));              // box a found offset as an integer result
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for the mixed bool box
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false for strpos() not found
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so offset 0 remains distinguishable from not found
            emitter.instruction(&format!("b {}", end_label));                   // skip the integer boxing path after the not-found result
            emitter.label(&found_label);
            emitter.instruction("mov x1, x0");                                  // move the found offset into the mixed helper payload register
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads do not use a high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int for strpos() found offsets
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the found integer offset as mixed
            emitter.label(&end_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // distinguish a valid non-negative match offset from the not-found sentinel
            emitter.instruction(&format!("jge {}", found_label));               // box a found offset as an integer result
            emitter.instruction("xor edi, edi");                                // false payload = 0 for the mixed bool box
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false for strpos() not found
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so offset 0 remains distinguishable from not found
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the integer boxing path after the not-found result
            emitter.label(&found_label);
            emitter.instruction("mov rdi, rax");                                // move the found offset into the mixed helper payload register
            emitter.instruction("xor esi, esi");                                // integer mixed payloads do not use a high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int for strpos() found offsets
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the found integer offset as mixed
            emitter.label(&end_label);
        }
    }
}
