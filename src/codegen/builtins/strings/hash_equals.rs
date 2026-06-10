//! Purpose:
//! Emits PHP `hash_equals($known, $user)` calls — a timing-safe string equality
//! check. Marshals the two string arguments into the shared two-string ABI and
//! calls the pure `__rt_hash_equals` runtime helper (no crypto library).
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Same two-string register convention as `str_contains`/`__rt_strpos`; the
//!   runtime returns the PHP boolean (0/1) directly in the int-result register.

use super::args::emit_string_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `hash_equals($known, $user)` builtin call.
///
/// Evaluates the known string, preserves it while evaluating the user string,
/// materialises both into the two-string ABI registers, and calls
/// `__rt_hash_equals` which performs a constant-time comparison and returns a
/// PHP boolean directly. Returns `PhpType::Bool`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_equals()");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push the known string ptr/len while evaluating the user string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");                                  // move the user string pointer into the third comparison argument register
            emitter.instruction("mov x4, x2");                                  // move the user string length into the fourth comparison argument register
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the known string ptr/len after evaluating the user string
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // save the known string ptr/len while evaluating the user string
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rcx, rdx");                                // move the user string length into the fourth SysV comparison argument register
            emitter.instruction("mov rdx, rax");                                // move the user string pointer into the third SysV comparison argument register
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the known string ptr/len into the first two SysV argument registers
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_equals");                          // constant-time compare; returns the PHP boolean (0/1) directly
    Some(PhpType::Bool)
}
