//! Purpose:
//! Emits PHP `grapheme_strrev` calls and boxes the `string|false` result shape.
//! Keeps grapheme-aware reversal separate from byte-wise `strrev` lowering.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - The runtime helper returns a string pointer/length pair on success or a null pointer on UTF-8 segmentation failure.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `grapheme_strrev` builtin.
///
/// The string argument is evaluated using the normal expression lowering. The
/// runtime helper reverses the input by UTF-8 grapheme clusters and returns a
/// raw string pair on success; this wrapper boxes that pair as `Mixed` so the
/// PHP `string|false` signature remains representable to callers.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("grapheme_strrev()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_grapheme_strrev");                     // reverse the input string by grapheme clusters and return string or failure sentinel
    box_grapheme_strrev_result(emitter, ctx);

    Some(PhpType::Mixed)
}

/// Boxes the raw runtime result as PHP `string|false`.
///
/// Success returns a non-null string pointer plus length, which is persisted and
/// boxed through `__rt_mixed_from_value`. Failure returns a null pointer and is
/// boxed as boolean false, preserving PHP's `string|false` observable surface.
fn box_grapheme_strrev_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("grapheme_strrev_false");
    let done_label = ctx.next_label("grapheme_strrev_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null string pointer means UTF-8 segmentation failed
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            emitter.instruction(&format!("b {}", done_label));                  // skip false boxing after a successful grapheme reversal
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for grapheme_strrev() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible string|false semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null string pointer means UTF-8 segmentation failed
            emitter.instruction(&format!("jz {}", false_label));                // box false when the runtime reports segmentation failure
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            emitter.instruction(&format!("jmp {}", done_label));                // skip false boxing after a successful grapheme reversal
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for grapheme_strrev() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible string|false semantics
            emitter.label(&done_label);
        }
    }
}
