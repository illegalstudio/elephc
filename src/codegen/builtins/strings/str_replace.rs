//! Purpose:
//! Emits PHP `str_replace` string transformation or formatting calls.
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

/// Emits codegen for the PHP `str_replace(search, replacement, subject)` builtin call.
///
/// `args[0]` = search string, `args[1]` = replacement string, `args[2]` = subject string.
/// Each string argument is emitted as a pointer/length pair in ABI registers.
/// Stack-based preservation pattern: search is saved first, then replacement, then subject
/// is evaluated; registers are restored so the runtime helper receives search in the primary
/// pair, replacement in the secondary pair, and subject in the third pair.
/// Calls `__rt_str_replace` and returns `PhpType::Str`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("str_replace()");
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the search string while evaluating the replacement and subject strings
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the replacement string while evaluating the subject string
            super::args::emit_string_arg(&args[2], emitter, ctx, data);
            emitter.instruction("mov x5, x1");                                  // move the subject pointer into the third runtime string-argument pair
            emitter.instruction("mov x6, x2");                                  // move the subject length into the third runtime string-argument pair
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the replacement string into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the search string into the primary runtime string-argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the search string while evaluating the replacement and subject strings
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the replacement string while evaluating the subject string
            super::args::emit_string_arg(&args[2], emitter, ctx, data);
            emitter.instruction("mov rcx, rax");                                // move the subject pointer into the third x86_64 runtime string-argument pair
            emitter.instruction("mov r8, rdx");                                 // move the subject length into the third x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the replacement string into the secondary x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the search string into the primary x86_64 string-helper input registers
        }
    }
    abi::emit_call_label(emitter, "__rt_str_replace");                          // replace every search-string occurrence inside the subject through the target-aware runtime helper

    Some(PhpType::Str)
}
