//! Purpose:
//! Emits PHP `get_resource_id` calls.
//! Returns the 1-based resource id — the descriptor plus one.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Reuses the shared `emit_stream_fd_arg` helper, then adds one so the id
//!   matches elephc's 1-based `Resource id #N` display.

use crate::codegen::builtins::io::stream_arg::emit_stream_fd_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("get_resource_id()");
    // The helper validates the argument and leaves the descriptor in the
    // integer result register; elephc's resource id is the descriptor plus
    // one, matching the 1-based "Resource id #N" display.
    emit_stream_fd_arg("get_resource_id", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x0, x0, #1");                              // descriptor -> 1-based resource id
        }
        Arch::X86_64 => {
            emitter.instruction("add rax, 1");                                  // descriptor -> 1-based resource id
        }
    }
    Some(PhpType::Int)
}
