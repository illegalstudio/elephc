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
    emitter.comment("min()");

    // -- evaluate first arg --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    let mut any_float = t0 == PhpType::Float;

    // -- check all arg types for float promotion --
    // We need to know upfront if any arg is float so we use a consistent register
    // For simplicity, we'll track float dynamically per pair

    for (i, arg) in args.iter().enumerate().skip(1) {
        // -- push current minimum onto stack --
        if any_float {
            if i == 1 && t0 != PhpType::Float {
                abi::emit_int_result_to_float_result(emitter);                  // normalize the first min() operand into the active floating-point result register before it becomes the running floating minimum
            }
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));  // preserve the current floating minimum while the next candidate expression is evaluated
        } else {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // preserve the current integer minimum while the next candidate expression is evaluated
        }

        let ti = emit_expr(arg, emitter, ctx, data);

        if any_float || ti == PhpType::Float {
            // -- float comparison path --
            if ti != PhpType::Float {
                abi::emit_int_result_to_float_result(emitter);                  // normalize the new min() candidate into the active floating-point result register before the floating comparison
            }
            if !any_float {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        abi::emit_pop_reg(emitter, "x9");                       // restore the previous integer minimum before promoting it into the floating comparison path
                        emitter.instruction("scvtf d1, x9");                    // convert the previous integer minimum into the secondary AArch64 floating-point scratch register
                    }
                    Arch::X86_64 => {
                        abi::emit_pop_reg(emitter, "r9");                       // restore the previous integer minimum before promoting it into the floating comparison path
                        emitter.instruction("cvtsi2sd xmm1, r9");               // convert the previous integer minimum into the secondary x86_64 floating-point scratch register
                    }
                }
            } else {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        abi::emit_pop_float_reg(emitter, "d1");                 // restore the previous floating minimum into the secondary AArch64 floating-point scratch register
                    }
                    Arch::X86_64 => {
                        abi::emit_pop_float_reg(emitter, "xmm1");               // restore the previous floating minimum into the secondary x86_64 floating-point scratch register
                    }
                }
            }
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("fmin d0, d1, d0");                     // compute the smaller of the previous and new floating candidates in the standard AArch64 floating-point result register
                }
                Arch::X86_64 => {
                    emitter.instruction("minsd xmm1, xmm0");                    // compute the smaller of the previous and new floating candidates in the secondary x86_64 floating-point scratch register
                    emitter.instruction("movsd xmm0, xmm1");                    // move the updated floating minimum back into the standard x86_64 floating-point result register
                }
            }
            any_float = true;
        } else {
            // -- integer comparison path --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x1, [sp], #16");                   // restore the previous integer minimum into the AArch64 scratch register before the scalar comparison
                    emitter.instruction("cmp x1, x0");                          // compare the previous integer minimum against the new integer candidate
                    emitter.instruction("csel x0, x1, x0, lt");                 // select the smaller integer value into the standard AArch64 integer result register
                }
                Arch::X86_64 => {
                    abi::emit_pop_reg(emitter, "r9");                           // restore the previous integer minimum into a scratch register before the scalar comparison
                    emitter.instruction("cmp r9, rax");                         // compare the previous integer minimum against the new integer candidate
                    emitter.instruction("cmovl rax, r9");                       // keep the smaller integer value in the standard x86_64 integer result register
                }
            }
        }
    }

    if any_float {
        Some(PhpType::Float)
    } else {
        Some(PhpType::Int)
    }
}
