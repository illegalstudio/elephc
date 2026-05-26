//! Purpose:
//! Emits PHP `fsync` stream builtin calls over runtime file handles.
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

/// Emits a call to the `fsync` runtime helper for the given file stream.
///
/// Unboxes the stream resource via `emit_stream_fd_arg` to extract its raw file
/// descriptor, then emits a call to `__rt_fsync` which wraps the libc `fsync(fd)`
/// call. Returns `PhpType::Bool` to reflect PHP's synchronous operation semantics.
///
/// # Arguments
/// - `args[0]` must be a valid stream resource; the function indexes without bounds
///   checking and relies on the type checker to validate argument count.
/// - `ctx` carries variable layout and ownership state; `emitter` receives the
///   generated assembly; `data` holds runtime data section entries.
///
/// # Returns
/// `Some(PhpType::Bool)` — fsync always returns a boolean in PHP.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fsync()");
    emit_stream_fd_arg("fsync", &args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fsync");                                // libc fsync(fd) wrapper
    Some(PhpType::Bool)
}
