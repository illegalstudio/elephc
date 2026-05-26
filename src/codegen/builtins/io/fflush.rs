//! Purpose:
//! Emits PHP `fflush` stream builtin calls over runtime file handles.
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

/// Emits the `fflush` builtin call, flushing the output buffer of an open file handle.
///
/// # Arguments
/// - `_name`: Unused name for dispatch; the builtin is identified by this module.
/// - `args`: Must contain at least one `Expr` identifying the stream resource.
/// - `emitter`: Target assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and stream metadata.
/// - `data`: Data section for constants and relocations.
///
/// # Behavior
/// Unboxes the stream resource via `emit_stream_fd_arg` to extract the raw file descriptor,
/// then calls `__rt_fflush` (a libc `fsync` wrapper with PHP-side fflush semantics).
///
/// # Return
/// Always returns `Some(PhpType::Bool)` — `true` on success, `false` on error (e.g., invalid stream).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fflush()");
    emit_stream_fd_arg("fflush", &args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fflush");                               // libc fsync(fd) wrapper (PHP-side fflush semantics)
    Some(PhpType::Bool)
}
