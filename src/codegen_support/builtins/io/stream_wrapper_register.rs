//! Purpose:
//! Emits PHP `stream_wrapper_register` calls.
//! Records a `(protocol, class-name)` pair in the runtime user-wrapper table
//! and returns the registration success boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - v1 stores up to 16 registrations in `_user_wrappers` and returns `true`;
//!   the wrapper class is not yet invoked by `fopen` (that integration is the
//!   next Phase-10 commit).
//! - The optional third `flags` argument is evaluated for its side effects
//!   and otherwise ignored.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_wrapper_register()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_wrapper_register()");
    // PHP evaluates the protocol string first, then the class string, then
    // the optional flags. The flags are accepted for compatibility and
    // discarded; the two strings are handed to the runtime helper.
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the protocol string
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the class string
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
            }
            abi::emit_pop_reg_pair(emitter, "x2", "x3"); // restore class ptr/len
            abi::emit_pop_reg_pair(emitter, "x0", "x1"); // restore protocol ptr/len
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the protocol string
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the class string
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
            }
            abi::emit_pop_reg_pair(emitter, "rdx", "rcx"); // restore class ptr/len
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi"); // restore protocol ptr/len
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_wrapper_register");
    Some(PhpType::Bool)
}
