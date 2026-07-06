//! Purpose:
//! Emits PHP `ord` string transformation or formatting calls.
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
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `ord()` builtin, which returns the ASCII/UTF-8 code point
/// of the first character in a string argument.
///
/// # Arguments
/// - `_name`: Unused, always "ord" (kept for interface uniformity with other builtins).
/// - `args`: Single argument — the string to extract the first code point from.
/// - `emitter`: Target-aware instruction emitter.
/// - `ctx`: Codegen context providing label generation and architecture info.
/// - `data`: Data section for relocations (unused by this builtin).
///
/// # Returns
/// `Some(PhpType::Int)` — the numeric code point of the first character.
/// Returns 0 for empty strings (matching PHP behavior).
///
/// # Architecture handling
/// - **AArch64**: Expects string pointer in `x1`, length in `x2`, returns result in `x0`.
/// - **x86_64**: Expects string pointer in `rax`, length in `rdx`, returns result in `eax`.
/// - Both targets set the integer register to 0 when the string is empty.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ord()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    let empty_label = ctx.next_label("ord_empty");
    let done_label = ctx.next_label("ord_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x2, {empty_label}"));             // return zero when ord() receives an empty string
            emitter.instruction("ldrb w0, [x1]");                               // load the first byte of the string as an unsigned integer code point
            emitter.instruction(&format!("b {done_label}"));                    // skip the empty-string fallback after loading the first byte
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // return zero when ord() receives an empty string
            emitter.instruction(&format!("jz {empty_label}"));                  // branch to the empty-string fallback when the string length is zero
            emitter.instruction("movzx eax, BYTE PTR [rax]");                   // load the first byte of the string as an unsigned integer code point
            emitter.instruction(&format!("jmp {done_label}"));                  // skip the empty-string fallback after loading the first byte
        }
    }
    emitter.label(&empty_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // return zero when ord() receives an empty string
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // return zero when ord() receives an empty string
        }
    }
    emitter.label(&done_label);

    Some(PhpType::Int)
}
