//! Purpose:
//! Emits PHP `ucfirst` string transformation or formatting calls.
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

/// Emits the PHP `ucfirst` builtin.
///
/// Evaluates `args[0]` as a string expression, copies it via `__rt_strcopy`, then
/// uppercases the first byte in-place if it falls in the ASCII lowercase range ('a'-'z').
/// Returns `PhpType::Str`.
///
/// # Arguments
/// * `_name` — unused; the builtin name is always `ucfirst`
/// * `args` — must contain exactly one string/scalar argument
/// * `emitter` — target-aware instruction emitter
/// * `ctx` — variable layout and metadata context
/// * `data` — data section for relocations and constants
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ucfirst()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    // -- copy string then uppercase the first character --
    abi::emit_call_label(emitter, "__rt_strcopy");                              // copy the source string into concat storage before mutating its first byte in place
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x2, 1f");                                  // skip the ASCII-case tweak when ucfirst() receives an empty string
            emitter.instruction("ldrb w9, [x1]");                               // load the first byte of the copied string so ucfirst() can classify its ASCII case
            emitter.instruction("cmp w9, #97");                                 // compare the copied first byte against 'a' to detect lowercase ASCII input
            emitter.instruction("b.lt 1f");                                     // leave bytes below 'a' unchanged because they are not lowercase ASCII letters
            emitter.instruction("cmp w9, #122");                                // compare the copied first byte against 'z' to bound the lowercase ASCII range
            emitter.instruction("b.gt 1f");                                     // leave bytes above 'z' unchanged because they are not lowercase ASCII letters
            emitter.instruction("sub w9, w9, #32");                             // convert lowercase ASCII to uppercase by subtracting the standard ASCII case delta
            emitter.instruction("strb w9, [x1]");                               // store the uppercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // skip the ASCII-case tweak when ucfirst() receives an empty string
            emitter.instruction("jz 1f");                                       // leave empty strings unchanged because there is no first byte to uppercase
            emitter.instruction("movzx ecx, BYTE PTR [rax]");                   // load the first byte of the copied string so ucfirst() can classify its ASCII case
            emitter.instruction("cmp cl, 97");                                  // compare the copied first byte against 'a' to detect lowercase ASCII input
            emitter.instruction("jb 1f");                                       // leave bytes below 'a' unchanged because they are not lowercase ASCII letters
            emitter.instruction("cmp cl, 122");                                 // compare the copied first byte against 'z' to bound the lowercase ASCII range
            emitter.instruction("ja 1f");                                       // leave bytes above 'z' unchanged because they are not lowercase ASCII letters
            emitter.instruction("sub cl, 32");                                  // convert lowercase ASCII to uppercase by subtracting the standard ASCII case delta
            emitter.instruction("mov BYTE PTR [rax], cl");                      // store the uppercased first byte back into the copied string in concat storage
            emitter.raw("1:");
        }
    }

    Some(PhpType::Str)
}
