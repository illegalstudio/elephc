//! Purpose:
//! Emits PHP `popen` calls.
//! Opens a process pipe and yields it as a PHP stream resource.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_popen` helper returns the pipe descriptor or -1; the result is
//!   boxed by the shared `box_socket_result` helper as `resource|false`.

use crate::codegen_support::builtins::io::stream_socket_server::box_socket_result;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `popen()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("popen()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the command string while the mode string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // mode pointer becomes the third helper argument
            emitter.instruction("mov x4, x2");                                  // mode length becomes the fourth helper argument
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the command string into the first two helper arguments
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the command string while the mode string is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // mode length becomes the fourth SysV helper argument
            emitter.instruction("mov rdx, rax");                                // mode pointer becomes the third SysV helper argument
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the command string into the first two SysV helper arguments
        }
    }
    abi::emit_call_label(emitter, "__rt_popen");
    box_socket_result(emitter, ctx);
    Some(PhpType::Mixed)
}
