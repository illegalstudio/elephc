//! Purpose:
//! Emits PHP `sscanf` string transformation or formatting calls.
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

/// Emits the `sscanf` builtin call.
///
/// `sscanf($string, $format)` parses the input string according to a format specifier
/// and returns an array of matched values as strings. This emitter evaluates the input
/// string first, then the format string, before invoking the `__rt_sscanf` runtime helper.
///
/// Arguments:
/// - `args[0]`: the input string to parse (string pointer in x1/rax, length in x2/rdx)
/// - `args[1]`: the format specifier string (string pointer in x1/rax, length in x2/rdx)
///
/// ABI behavior:
/// - AArch64: saves input string to stack via `stp x1, x2, [sp, #-16]!`, emits format args into x3/x4, restores input via `ldp x1, x2, [sp], #16`
/// - X86_64: saves input string via `push_reg_pair` to stack, emits format args into rdi/rsi, restores input via `pop_reg_pair`
///
/// Returns: `Some(PhpType::Array(Box::new(PhpType::Str)))` indicating an array of strings.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sscanf()");
    // sscanf($string, $format) → returns array of matched values as strings
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the input string while the format string expression is evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the format pointer into the secondary runtime string-argument pair
            emitter.instruction("mov x4, x2");                                  // move the format length into the secondary runtime string-argument pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the input string into the primary runtime string-argument pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // push the input string while the format string expression is evaluated
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // move the format pointer into the secondary x86_64 runtime string-argument pair
            emitter.instruction("mov rsi, rdx");                                // move the format length into the secondary x86_64 runtime string-argument pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the input string into the primary x86_64 runtime string-argument pair
        }
    }
    abi::emit_call_label(emitter, "__rt_sscanf");                               // parse the input string according to the format string through the target-aware runtime helper
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
