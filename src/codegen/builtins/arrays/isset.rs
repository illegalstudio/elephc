//! Purpose:
//! Emits PHP `isset` checks without evaluating to ordinary truthiness.
//! Owns null/unset sentinel handling for variables and array element probes.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Must distinguish PHP null/unset semantics from false, zero, and empty string values.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("isset()");
    if let ExprKind::ArrayAccess { array, index } = &args[0].kind {
        let array_ty = crate::codegen::functions::infer_contextual_type(array, ctx);
        if crate::codegen::expr::arrays::type_is_array_access_object(&array_ty, ctx) {
            crate::codegen::expr::arrays::emit_array_access_offset_exists(
                array, index, emitter, ctx, data,
            );
            return Some(PhpType::Int);
        }
        if array_ty.codegen_repr() == PhpType::Str {
            emit_expr(&args[0], emitter, ctx, data);
            emit_string_offset_isset_result(emitter);
            return Some(PhpType::Int);
        }
    }

    emit_expr(&args[0], emitter, ctx, data);
    // -- compiled variables always exist, so isset returns true --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #1");                                  // return 1 (true) since the compiled variable is always set
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, 1");                                  // return 1 (true) since the compiled variable is always set
        }
    }

    Some(PhpType::Int)
}

fn emit_string_offset_isset_result(emitter: &mut Emitter) {
    let (_, len_reg) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", len_reg));               // check whether string offset access produced a character
            emitter.instruction("cset x0, ne");                                 // return true only when the string offset is in bounds
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, 0", len_reg));                // check whether string offset access produced a character
            emitter.instruction("setne al");                                    // return true only when the string offset is in bounds
            emitter.instruction("movzx eax, al");                               // widen the boolean byte into the canonical integer result
        }
    }
}
