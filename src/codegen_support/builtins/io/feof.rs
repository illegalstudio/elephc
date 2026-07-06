//! Purpose:
//! Emits PHP `feof` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code for the PHP `feof(stream)` builtin.
///
/// Unboxes the stream resource in `args[0]` to extract a raw file descriptor,
/// then calls the target-aware runtime helper `__rt_feof` via the platform ABI.
/// Returns `PhpType::Bool` indicating end-of-file status.
///
/// # Arguments
/// - `args[0]`: must be a valid stream expression (validated by type checker).
/// - `emitter`: used for instruction emission and target awareness.
/// - `ctx`: carries variable layout and ownership state.
/// - `data`: used for any runtime data section emission required by stream unboxing.
///
/// # ABI details
/// - On x86_64: moves the file descriptor from `rax` (returned by stream unboxing) to `rdi`
///   before the call to satisfy the SysV AMD64 ABI first-argument register.
/// - On ARM64: the file descriptor is already in the correct register per the ABI contract.
///
/// # Return
/// `Some(PhpType::Bool)` — `feof` always returns a boolean in PHP.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("feof()");
    emit_stream_fd_arg("feof", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the file descriptor into the first SysV feof helper argument register
    }
    abi::emit_call_label(emitter, "__rt_feof");                                 // query the target-aware eof helper for the given file descriptor
    Some(PhpType::Bool)
}
