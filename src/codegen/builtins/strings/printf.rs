//! Purpose:
//! Emits PHP `printf` string formatting calls (`sprintf` + write to stdout).
//! Marshals string/scalar arguments into the shared sprintf runtime helper, then writes the
//! formatted bytes to stdout.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.
//! - Argument marshalling (including coercing each argument to its conversion specifier's type for
//!   literal formats) is shared with `sprintf` via `super::format_args`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `printf` builtin call.
///
/// Implements `printf` as `sprintf` + `echo`: delegates argument marshalling and the
/// `__rt_sprintf` call to [`super::format_args::emit_format_and_call`], then writes the formatted
/// string (returned in the standard string-result registers) to stdout via the target write
/// syscall and returns the byte count written.
///
/// # Arguments
/// - `_name`: unused for `printf`; required by the dispatcher signature
/// - `args`: `[format_string, arg1, arg2, ...]` — format string is always `args[0]`
/// - `emitter`: target-aware instruction emitter
/// - `ctx`: codegen context (variable layout, ownership state, class metadata)
/// - `data`: data section for relocations and static data
///
/// # Returns
/// `Some(PhpType::Int)` — always returns the character count written (PHP printf semantics).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("printf()");

    // printf = sprintf + echo: marshal args and format the string (result ptr/len in the
    // standard string-result registers: x1/x2 on ARM64, rax/rdx on x86_64).
    super::format_args::emit_format_and_call(args, emitter, ctx, data);

    // -- write result to stdout --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
            emitter.instruction("mov x0, x2");                                  // return char count
        }
        Arch::X86_64 => {
            emitter.instruction("mov r8, rdx");                                 // preserve the byte count in r8; the syscall instruction clobbers rcx
            emitter.instruction("mov rsi, rax");                                // move the formatted string pointer into the SysV write buffer register
            emitter.instruction("mov rdx, r8");                                 // move the formatted string length into the SysV write byte-count register
            emitter.instruction("mov edi, 1");                                  // fd = stdout for the Linux x86_64 write syscall
            emitter.instruction("mov eax, 1");                                  // syscall 1 = write on Linux x86_64
            emitter.instruction("syscall");                                     // write the formatted bytes to stdout through the Linux x86_64 syscall ABI
            emitter.instruction("mov rax, r8");                                 // return the byte count (rcx was destroyed by syscall)
        }
    }

    Some(PhpType::Int)
}
