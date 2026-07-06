//! Purpose:
//! Emits PHP `stream_wrapper_unregister` calls.
//! Removes a user-defined wrapper registration from the runtime
//! `_user_wrappers` table.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Returns `true` when a matching slot was cleared and `false` when the
//!   protocol was not registered. Built-in protocols (`file`, `php`, ...) are
//!   not user-registered and cannot be unregistered in v1.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_wrapper_unregister()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_wrapper_unregister()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // arg 0 = protocol pointer
            emitter.instruction("mov x1, x2");                                  // arg 1 = protocol length
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // arg 0 = protocol pointer
            emitter.instruction("mov rsi, rdx");                                // arg 1 = protocol length
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_wrapper_unregister");
    Some(PhpType::Bool)
}
