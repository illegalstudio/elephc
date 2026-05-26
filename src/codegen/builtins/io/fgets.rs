//! Purpose:
//! Emits PHP `fgets` stream builtin calls over runtime file handles.
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
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits a call to the `fgets` builtin.
///
/// Unboxes the stream resource in `args[0]` to extract a raw file descriptor,
/// then invokes `__rt_fgets` to read one line from the stream. On x86_64 the
/// file descriptor is moved to `rdi` (SysV ABI) before the call.
///
/// # Arguments
/// * `args[0]` — must be a valid stream resource; validated by `emit_stream_fd_arg`.
/// * `emitter` — target-aware instruction emitter.
/// * `ctx` — codegen context carrying stream/FD metadata.
///
/// # Returns
/// `Some(PhpType::Str)` on success (the read line as an owned string);
/// execution halts on error (caller handles fatal).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fgets()");
    emit_stream_fd_arg("fgets", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the file descriptor into the first SysV fgets helper argument register
    }
    abi::emit_call_label(emitter, "__rt_fgets");                                // read one line through the target-aware runtime helper and return it as an elephc string
    Some(PhpType::Str)
}
