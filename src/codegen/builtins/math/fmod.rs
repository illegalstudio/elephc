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
    emitter.comment("fmod()");
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the dividend to float when the first fmod() argument is an integer
            }
        }
        Arch::X86_64 => {
            if t0 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the dividend to float when the first fmod() argument is an integer
            }
        }
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the dividend while the divisor expression is evaluated
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t1 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the divisor to float when the second fmod() argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "d1");                             // restore the dividend into the left-hand floating-point scratch register
            emitter.instruction("fdiv d2, d1, d0");                             // compute dividend / divisor in a temporary floating-point register
            emitter.instruction("frintz d2, d2");                               // truncate the quotient toward zero to match PHP/C fmod semantics
            emitter.instruction("fmsub d0, d2, d0, d1");                        // compute dividend - trunc(dividend/divisor) * divisor as the floating remainder
        }
        Arch::X86_64 => {
            if t1 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the divisor to float when the second fmod() argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the dividend into the left-hand floating-point argument register
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the divisor while rearranging the floating-point libc fmod() arguments
            emitter.instruction("movapd xmm0, xmm1");                           // move the dividend into the first libc fmod() floating-point argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the divisor into the second libc fmod() floating-point argument register
            emitter.instruction("call fmod");                                   // delegate the floating remainder semantics to libc fmod() on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
