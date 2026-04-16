use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("pi()");
    let label = data.add_float(std::f64::consts::PI);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x9", &format!("{}", label));                           // load the page address that contains the M_PI floating constant
            emitter.ldr_lo12("d0", "x9", &format!("{}", label));                // load the M_PI floating constant into the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd xmm0, QWORD PTR [rip + {}]", label)); // load the M_PI floating constant into the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
