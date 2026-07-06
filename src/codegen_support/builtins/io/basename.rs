//! Purpose:
//! Emits PHP `basename` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `basename(path, suffix?)` builtin.
///
/// Handles both single-argument and two-argument forms. The path string is
/// passed in the primary string-argument register pair (x1/x2 on AArch64, rax/rdx
/// on x86_64). If a suffix is provided, it is evaluated after the path and placed
/// in the secondary string-argument pair (x3/x4 or rdi/rsi) before restoring the
/// path pair. When no suffix is supplied, null registers signal "no suffix" to
/// the runtime helper.
///
/// Calls `__rt_basename` and returns `Some(PhpType::Str)` on success, or
/// propagates a runtime error on failure.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("basename()");
    emit_expr(&args[0], emitter, ctx, data);
    if args.len() >= 2 {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the path ptr/len while the suffix expression is evaluated
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov x3, x1");                              // move the suffix pointer into the secondary runtime string-argument pair
                emitter.instruction("mov x4, x2");                              // move the suffix length into the secondary runtime string-argument pair
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the path ptr/len after evaluating the suffix expression
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(emitter, "rax", "rdx");                 // preserve the path ptr/len while the suffix expression is evaluated
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the suffix pointer into the x86_64 secondary runtime string-argument slot
                emitter.instruction("mov rsi, rdx");                            // move the suffix length into the x86_64 secondary runtime string-argument slot
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                  // restore the path ptr/len after evaluating the suffix expression
            }
        }
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x3, #0");                              // no suffix supplied: pointer = 0
                emitter.instruction("mov x4, #0");                              // no suffix supplied: length = 0 (runtime branches on this)
            }
            Arch::X86_64 => {
                emitter.instruction("xor edi, edi");                            // no suffix supplied: pointer = 0
                emitter.instruction("xor esi, esi");                            // no suffix supplied: length = 0 (runtime branches on this)
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_basename");                             // call the target-aware runtime helper that returns the trailing name component
    Some(PhpType::Str)
}
