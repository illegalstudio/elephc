//! Purpose:
//! Emits PHP `fpassthru` stream builtin calls over runtime file handles.
//! Validates the stream argument before streaming remaining bytes to stdout.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returns the total number of bytes copied to stdout, or -1 on read failure.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fpassthru()");
    emit_stream_fd_arg("fpassthru", &args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fpassthru");                            // call the runtime helper that streams remaining bytes of the fd to stdout
    Some(PhpType::Int)
}
