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
    emitter.comment("pow()");
    // -- evaluate base, save it, evaluate exponent, call C pow() --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the pow() base to float when the first argument is an integer
            }
        }
        Arch::X86_64 => {
            if t0 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the pow() base to float when the first argument is an integer
            }
        }
    }
    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));          // preserve the floating pow() base while the exponent expression is evaluated
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            if t1 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert the pow() exponent to float when the second argument is an integer
            }
            emitter.instruction("fmov d1, d0");                                 // move the floating exponent into the second libc pow() argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating base into the first libc pow() argument register
            emitter.bl_c("pow");                                                // delegate the exponentiation to libc pow() on AArch64
        }
        Arch::X86_64 => {
            if t1 != PhpType::Float {
                emitter.instruction("cvtsi2sd xmm0, rax");                      // convert the pow() exponent to float when the second argument is an integer
            }
            abi::emit_pop_float_reg(emitter, "xmm1");                           // restore the floating base into a scratch floating-point register before ordering the SysV libc pow() arguments
            emitter.instruction("movapd xmm2, xmm0");                           // preserve the floating exponent while the floating base is moved into the first libc pow() argument register
            emitter.instruction("movapd xmm0, xmm1");                           // move the floating base into the first libc pow() argument register
            emitter.instruction("movapd xmm1, xmm2");                           // move the floating exponent into the second libc pow() argument register
            emitter.instruction("call pow");                                    // delegate the exponentiation to libc pow() on linux-x86_64
        }
    }
    Some(PhpType::Float)
}
