//! Purpose:
//! Emits PHP `chmod` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `chmod` builtin call.
///
/// Evaluates the path argument first (ptr in x1, len in x2 on ARM64; rax/rdx on x86_64),
/// preserves it on the stack while evaluating the mode argument, then calls `__rt_chmod`.
///
/// Arguments:
/// - `args[0]`: path (string)
/// - `args[1]`: mode (integer octal, e.g. 0o755)
///
/// Returns: `PhpType::Bool` (true on success, false on failure, matching PHP semantics)
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("chmod()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path ptr/len while the mode expression is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // move mode value into the runtime's mode register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path ptr/len
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // mode → secondary integer arg slot
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore path ptr/len
        }
    }
    abi::emit_call_label(emitter, "__rt_chmod");                                // call the target-aware runtime helper that wraps libc chmod()
    Some(PhpType::Bool)
}
