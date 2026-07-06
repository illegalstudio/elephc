//! Purpose:
//! Emits PHP `readline` file input builtin calls.
//! Coordinates path or stream arguments with runtime helpers that allocate returned strings or arrays.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Failure paths must distinguish PHP false from empty string or empty array results.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `readline([prompt])` builtin.
///
/// Takes one optional argument: the prompt string to write to stdout before reading.
/// When a prompt is provided, emits a `write` syscall (AArch64) or libc `write` call (x86_64)
/// to stdout before reading. Always reads one line from stdin via `__rt_fgets`.
///
/// Returns `Some(PhpType::Str)` with the line excluding the trailing newline.
/// The runtime helper distinguishes PHP `false` (on EOF) from empty string.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("readline()");
    if args.len() == 1 {
        emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // fd = stdout
                emitter.syscall(4);                                             // write the prompt string to stdout before reading from stdin
            }
            Arch::X86_64 => {
                emitter.instruction("mov rsi, rax");                            // move the prompt string pointer into the second SysV libc write() argument register
                emitter.instruction("mov rdi, 1");                              // pass stdout as the destination file descriptor for the prompt write
                emitter.instruction("call write");                              // write the prompt string through libc write() before reading from stdin
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // fd = stdin (fd 0)
        }
        Arch::X86_64 => {
            emitter.instruction("xor edi, edi");                                // fd = stdin (fd 0) in the first SysV runtime-helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_fgets");                                // read one line from stdin through the target-aware stream helper
    Some(PhpType::Str)
}
