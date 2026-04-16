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
    emitter.comment("deg2rad()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the degree input into the active floating-point result register before applying the conversion factor
    }
    // -- multiply by M_PI / 180.0 to convert degrees to radians --
    let label = data.add_float(std::f64::consts::PI / 180.0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x9", &format!("{}", label));                           // load the page address that contains the degree-to-radian conversion constant
            emitter.ldr_lo12("d1", "x9", &format!("{}", label));                // load the degree-to-radian conversion constant into the secondary AArch64 floating-point register
            emitter.instruction("fmul d0, d0, d1");                             // multiply the degree input by the conversion constant in the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", label)); // load the degree-to-radian conversion constant into the secondary x86_64 floating-point register
            emitter.instruction("mulsd xmm0, xmm1");                            // multiply the degree input by the conversion constant in the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
