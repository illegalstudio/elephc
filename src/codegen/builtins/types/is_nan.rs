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
    emitter.comment("is_nan()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- NaN is the only value that does not equal itself --
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer inputs into the active floating-point result register before the NaN check
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // compare the floating-point value against itself so NaN sets the unordered flag
            emitter.instruction("cset x0, vs");                                 // materialize the unordered NaN comparison result as a boolean integer
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // compare the floating-point value against itself so NaN sets the parity flag
            emitter.instruction("setp al");                                     // materialize the unordered NaN comparison result into the low boolean byte
            emitter.instruction("movzx rax, al");                               // widen the NaN boolean byte into the canonical integer result register
        }
    }
    Some(PhpType::Bool)
}
