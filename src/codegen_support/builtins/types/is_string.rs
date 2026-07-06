//! Purpose:
//! Emits PHP `is_string` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `is_string` type predicate as a runtime check.
///
/// For `PhpType::Mixed` or `PhpType::Union` arguments, unboxes the value via
/// `__rt_mixed_unbox` and compares the resulting runtime tag against the
/// string-payload sentinel (tag value 1). For types known at compile time,
/// returns a constant 1 (string) or 0 (not string).
///
/// Returns `Some(PhpType::Bool)` unconditionally.
///
/// Arguments:
/// - `args[0]`: the expression to test
///
/// Input type `ty`:
/// - `Mixed` / `Union`: runtime unbox + tag comparison
/// - `Str`: constant 1
/// - all other types: constant 0
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_string()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // Mixed/Union values are boxed cells — peel nested mixed wrappers
        // and compare the runtime tag against the string-payload tag.
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #1");                              // runtime tag 1 = string payload
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is a string, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 1");                              // runtime tag 1 = string payload
                emitter.instruction("sete al");                                 // set al when the unboxed payload is a string
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        // Compile-time type fully determines the answer for non-mixed types.
        let val = if matches!(ty, PhpType::Str) { 1 } else { 0 };
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("mov x0, #{}", val));              // set result: 1 if string, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, {}", val));              // set result: 1 if string, 0 otherwise
            }
        }
    }
    Some(PhpType::Bool)
}
