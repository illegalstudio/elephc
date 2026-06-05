//! Purpose:
//! Emits PHP `lcfirst` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `lcfirst` builtin call.
///
/// Emits `args[0]` as a string expression, then copies the result into concat
/// storage and lowercases its first byte in place when that byte is an uppercase
/// ASCII letter (A–Z). Non-ASCII or non-uppercase first bytes are left unchanged.
/// Returns `PhpType::Str` as the result type.
///
/// - **args**: single expression producing the input string (panics if empty)
/// - **emitter**: target-specific instruction emission
/// - **ctx**: codegen context carrying variable layout and metadata
/// - **data**: data section for relocatable string constants
/// - **returns**: `Some(PhpType::Str)` indicating the builtin produces a string
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("lcfirst()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    // -- copy string then lowercase the first character --
    abi::emit_call_label(emitter, "__rt_strcopy");                              // copy the source string into concat storage before mutating its first byte in place
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x2, 1f");                                  // skip the ASCII-case tweak when lcfirst() receives an empty string
            emitter.instruction("ldrb w9, [x1]");                               // load the first byte of the copied string so lcfirst() can classify its ASCII case
            emitter.instruction("cmp w9, #65");                                 // compare the copied first byte against 'A' to detect uppercase ASCII input
            emitter.instruction("b.lt 1f");                                     // leave bytes below 'A' unchanged because they are not uppercase ASCII letters
            emitter.instruction("cmp w9, #90");                                 // compare the copied first byte against 'Z' to bound the uppercase ASCII range
            emitter.instruction("b.gt 1f");                                     // leave bytes above 'Z' unchanged because they are not uppercase ASCII letters
            emitter.instruction("add w9, w9, #32");                             // convert uppercase ASCII to lowercase by adding the standard ASCII case delta
            emitter.instruction("strb w9, [x1]");                               // store the lowercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // skip the ASCII-case tweak when lcfirst() receives an empty string
            emitter.instruction("jz 1f");                                       // leave empty strings unchanged because there is no first byte to lowercase
            emitter.instruction("movzx ecx, BYTE PTR [rax]");                   // load the first byte of the copied string so lcfirst() can classify its ASCII case
            emitter.instruction("cmp cl, 65");                                  // compare the copied first byte against 'A' to detect uppercase ASCII input
            emitter.instruction("jb 1f");                                       // leave bytes below 'A' unchanged because they are not uppercase ASCII letters
            emitter.instruction("cmp cl, 90");                                  // compare the copied first byte against 'Z' to bound the uppercase ASCII range
            emitter.instruction("ja 1f");                                       // leave bytes above 'Z' unchanged because they are not uppercase ASCII letters
            emitter.instruction("add cl, 32");                                  // convert uppercase ASCII to lowercase by adding the standard ASCII case delta
            emitter.instruction("mov BYTE PTR [rax], cl");                      // store the lowercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
    }

    Some(PhpType::Str)
}
