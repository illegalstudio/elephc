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
    emitter.comment("sqrt()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- convert int to float if needed, then compute square root --
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer sqrt() inputs into the active floating-point result register before the square-root operation
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fsqrt d0, d0");                                // compute the scalar square root in the native AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction("sqrtsd xmm0, xmm0");                           // compute the scalar square root in the native x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
