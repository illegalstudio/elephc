//! Purpose:
//! Emits PHP `is_string` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

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
