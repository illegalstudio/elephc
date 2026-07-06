//! Purpose:
//! Emits PHP `chr` string transformation or formatting calls.
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

/// Emits code for PHP's `chr(int $codepoint): string` function.
///
/// Converts an integer ASCII/code-point value to a single-character string.
/// The argument is evaluated and loaded into a register via `emit_int_arg`,
/// then the target-aware runtime helper `__rt_chr` is called, which writes
/// one byte into concat storage and returns it as a PHP string.
///
/// # Arguments
/// * `args[0]` — integer expression producing the character code
///
/// # Returns
/// `PhpType::Str` — a single-character PHP string
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chr()");
    super::args::emit_int_arg(&args[0], emitter, ctx, data);
    // -- convert ASCII code to single-character string --
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the integer character code into the first SysV runtime argument register before materializing the one-byte string
    }
    abi::emit_call_label(emitter, "__rt_chr");                                  // call the target-aware runtime helper that writes one byte into concat storage and returns it as a string

    Some(PhpType::Str)
}
