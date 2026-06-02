//! Purpose:
//! Emits PHP `disk_free_space` and `disk_total_space` calls.
//! Reports the available or total byte capacity of a filesystem.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Both delegate to the `__rt_disk_space` runtime helper, passing a mode
//!   selector; the helper returns a double (0.0 when `statfs` fails).

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    emit_expr(&args[0], emitter, ctx, data);
    let mode = i64::from(name == "disk_total_space");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", mode));                 // mode: 0 = free space, 1 = total space
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // path pointer into the helper argument register
            emitter.instruction(&format!("mov edi, {}", mode));                 // mode: 0 = free space, 1 = total space
        }
    }
    abi::emit_call_label(emitter, "__rt_disk_space");
    Some(PhpType::Float)
}
