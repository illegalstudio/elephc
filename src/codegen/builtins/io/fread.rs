//! Purpose:
//! Emits PHP `fread` stream builtin calls over runtime file handles.
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

/// Lowers the PHP `fread` builtin call to target assembly.
///
/// Arguments:
///   - `args[0]`: stream resource (validated via `emit_stream_fd_arg`)
///   - `args[1]`: byte count expression
///
/// Emits:
///   - Stream unboxing and fd preservation on stack while length is evaluated
///   - ABI-aligned argument materialization for `__rt_fread` (fd in arg0, length in arg1)
///   - Tail call to `__rt_fread`, which returns an owned PHP string in x0/x1 or x0=0 on error
///
/// Returns `Some(PhpType::Str)` unconditionally; caller must handle false/null from runtime.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fread()");
    emit_stream_fd_arg("fread", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the length expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the requested byte count into the fread helper length register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the fread helper fd register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the requested byte count into the second SysV fread helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV fread helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_fread");                                // read bytes through the target-aware runtime helper and return an elephc string
    Some(PhpType::Str)
}
