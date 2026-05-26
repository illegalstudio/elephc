//! Purpose:
//! Emits PHP `fputcsv` stream builtin calls over runtime file handles.
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

/// Emits a PHP `fputcsv(stream, fields, separator, enclosure, escape)` builtin call.
///
/// Validates `stream` via `emit_stream_fd_arg` to extract a raw file descriptor,
/// then preserves it on the stack while `fields` (args[1]) is evaluated as a
/// string-array expression. After evaluation, the array pointer is moved into the
/// second ABI argument register and the file descriptor is restored to the first
/// register before calling `__rt_fputcsv`. Returns `PhpType::Int` (bytes written
/// or false on failure).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fputcsv()");
    emit_stream_fd_arg("fputcsv", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the file descriptor while the string-array expression is evaluated
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the string-array pointer into the second runtime helper argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the file descriptor into the first runtime helper argument register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the string-array pointer into the second SysV fputcsv helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the file descriptor into the first SysV fputcsv helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_fputcsv");                              // write the string array as a CSV line through the target-aware runtime helper
    Some(PhpType::Int)
}
