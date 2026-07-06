//! Purpose:
//! Emits PHP `stream_get_meta_data` calls.
//! Yields the metadata associative array describing an open stream resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The descriptor is unboxed from the stream resource and handed to the
//!   `__rt_stream_get_meta_data` runtime helper, which builds a `{string =>
//!   mixed}` hash.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `stream_get_meta_data()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_get_meta_data()");
    emit_stream_fd_arg("stream_get_meta_data", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // descriptor into the runtime-helper argument register
    }
    abi::emit_call_label(emitter, "__rt_stream_get_meta_data");
    Some(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
