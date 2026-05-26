//! Purpose:
//! Emits PHP `system` process-control or shell execution builtin calls.
//! Marshals command/status arguments into runtime helpers with PHP-visible output and exit behavior.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Process calls are effectful and may terminate or emit output, so lowering must preserve evaluation order.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the PHP `system()` builtin call.
///
/// Executes a command string via the C `system()` libc call. The command
/// output is written directly to stdout by the C library. This function
/// evaluates the command argument, null-terminates it via `__rt_cstr`,
/// calls `system()`, and returns an empty string since output is already
/// streamed to stdout.
///
/// # Arguments
/// * `_name` — unused builtin name (the module dispatches by function name)
/// * `args` — must contain exactly one expression yielding the command string
///
/// # Return
/// Always returns `PhpType::Str` (empty string), matching PHP `system()` semantics.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("system()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- null-terminate and call libc system() which outputs directly to stdout --
    abi::emit_call_label(emitter, "__rt_cstr");                                 // null-terminate the command string through the target-aware C-string helper
    match emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the null-terminated command pointer in the SysV first-argument register
        }
    }
    emitter.bl_c("system");                                          // execute command, output goes to stdout
    // -- return empty string (system() returns last line, but we let stdout handle it) --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, #0");                                  // return empty string ptr (null) after the direct stdout system() call
            emitter.instruction("mov x2, #0");                                  // return empty string len = 0 after the direct stdout system() call
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 0");                                  // return empty string ptr (null) after the direct stdout system() call
            emitter.instruction("mov rdx, 0");                                  // return empty string len = 0 after the direct stdout system() call
        }
    }
    Some(PhpType::Str)
}
