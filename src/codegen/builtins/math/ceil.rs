use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ceil()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the ceil() input to float when it is an integer
            }
            emitter.instruction("frintp d0, d0");                               // round toward plus infinity on AArch64
        }
        Arch::X86_64 => {
            if ty != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the ceil() input to float when it is an integer
            }
            emitter.instruction("roundsd xmm0, xmm0, 2");                       // round toward plus infinity on x86_64 using SSE4.1 roundsd
        }
    }
    Some(PhpType::Float)
}
