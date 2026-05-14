//! Purpose:
//! Emits PHP `is_int` type predicate calls.
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
    emitter.comment("is_int()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // normalize boxed mixed payloads to their concrete runtime tag
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // runtime tag 0 = integer payload
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is an int, 0 otherwise
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 0");                              // runtime tag 0 = integer payload
                emitter.instruction("sete al");                                 // set al when the unboxed payload is an int
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        let val = if matches!(ty, PhpType::Int) { 1 } else { 0 };
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val); // return the compile-time type predicate result
    }
    Some(PhpType::Bool)
}
