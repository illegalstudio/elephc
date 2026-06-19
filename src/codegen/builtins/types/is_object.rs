//! Purpose:
//! Emits PHP `is_object` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Boxed Mixed object values use runtime tag 6.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `is_object` type predicate.
///
/// For boxed Mixed or Union values, unboxes the runtime payload and checks the
/// object tag. For concrete static types, folds the predicate to a boolean
/// constant.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_object()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #6");                              // runtime tag 6 = object payload
                emitter.instruction("cset x0, eq");                             // x0 = 1 if the unboxed payload is an object
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 6");                              // runtime tag 6 = object payload
                emitter.instruction("sete al");                                 // set al when the unboxed payload is an object
                emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
            }
        }
    } else {
        let val = matches!(ty, PhpType::Object(_));
        abi::emit_load_int_immediate(
            emitter,
            abi::int_result_reg(emitter),
            if val { 1 } else { 0 },
        );
    }
    Some(PhpType::Bool)
}
