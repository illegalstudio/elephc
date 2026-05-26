//! Purpose:
//! Emits PHP `fdatasync` stream builtin calls over runtime file handles.
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
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits the `fdatasync` builtin call.
///
/// Unboxes the stream resource in `args[0]` to extract its file descriptor,
/// then calls the runtime helper `__rt_fdatasync`. Returns `PhpType::Bool`.
///
/// # Arguments
/// - `args[0]`: must be a valid stream resource; failure is fatal like PHP.
/// - `emitter`: instruction emitter for the current function.
/// - `ctx`: codegen context with current function frame and variable layout.
/// - `data`: data section for relocations and constant pools.
///
/// # Returns
/// Always `Some(PhpType::Bool)` — fdatasync has no failure path in this emitter.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fdatasync()");
    emit_stream_fd_arg("fdatasync", &args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fdatasync");                            // libc fdatasync(fd) — falls back to fsync on Darwin
    Some(PhpType::Bool)
}
