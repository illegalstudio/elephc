//! Purpose:
//! Emits PHP `ftruncate` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for PHP `ftruncate($stream, $size)`.
///
/// Validates `$stream` via `emit_stream_fd_arg`, then evaluates `$size` and
/// loads both arguments into ABI registers before calling `__rt_ftruncate`.
/// Returns `PhpType::Bool` because `ftruncate` returns `true` on success or `false` on failure.
///
/// # Arguments
/// - `_name`: builtin name (unused, always `ftruncate`)
/// - `args[0]`: stream resource expression (validated and unboxed to file descriptor)
/// - `args[1]`: size expression (evaluated after fd is preserved)
/// - `emitter`: target for code emission and target metadata
/// - `ctx`: codegen context (variable layout, ownership state)
/// - `data`: data section for relocations and runtime symbols
///
/// # Register usage
/// - AArch64: fd in `x0`, size in `x1`
/// - X86_64: fd in `rax`, size in `rdx`
///
/// # Return
/// `Some(PhpType::Bool)` — `ftruncate` always returns a boolean in PHP
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ftruncate()");
    emit_stream_fd_arg("ftruncate", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve fd while size is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x1, x0");                                  // size → second runtime arg
            emitter.instruction("ldr x0, [sp], #16");                           // restore fd into the first runtime arg
        }
        Arch::X86_64 => {
            emitter.instruction("push rax");                                    // preserve fd
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdx, rax");                                // size → secondary integer arg slot
            emitter.instruction("pop rax");                                     // restore fd
        }
    }
    abi::emit_call_label(emitter, "__rt_ftruncate");                            // call libc ftruncate(fd, size) wrapper
    Some(PhpType::Bool)
}
