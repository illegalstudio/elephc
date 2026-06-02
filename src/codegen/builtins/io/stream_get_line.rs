//! Purpose:
//! Emits PHP `stream_get_line` calls.
//! Reads from a stream up to a byte budget or an ending delimiter.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals the descriptor, length, and optional ending delimiter into the
//!   four `__rt_stream_get_line` argument registers; the delimiter is consumed
//!   and stripped from the returned string.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
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
    emitter.comment("stream_get_line()");
    emit_stream_fd_arg("stream_get_line", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    emit_expr(&args[1], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the maximum length
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x3, x2");                              // ending-delimiter length into argument 3
                emitter.instruction("mov x2, x1");                              // ending-delimiter pointer into argument 2
                abi::emit_pop_reg(emitter, "x1"); // maximum length into argument 1
                abi::emit_pop_reg(emitter, "x0"); // descriptor into argument 0
            }
            Arch::X86_64 => {
                emitter.instruction("mov rcx, rdx");                            // ending-delimiter length into argument 3
                emitter.instruction("mov rdx, rax");                            // ending-delimiter pointer into argument 2
                abi::emit_pop_reg(emitter, "rsi"); // maximum length into argument 1
                abi::emit_pop_reg(emitter, "rdi"); // descriptor into argument 0
            }
        }
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x2, #0");                              // no ending-delimiter pointer
                emitter.instruction("mov x3, #0");                              // no ending-delimiter length
                abi::emit_pop_reg(emitter, "x1"); // maximum length into argument 1
                abi::emit_pop_reg(emitter, "x0"); // descriptor into argument 0
            }
            Arch::X86_64 => {
                emitter.instruction("xor edx, edx");                            // no ending-delimiter pointer
                emitter.instruction("xor ecx, ecx");                            // no ending-delimiter length
                abi::emit_pop_reg(emitter, "rsi"); // maximum length into argument 1
                abi::emit_pop_reg(emitter, "rdi"); // descriptor into argument 0
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_get_line");
    Some(PhpType::Str)
}
