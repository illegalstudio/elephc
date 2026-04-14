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
    emitter.comment("round()");

    if args.len() == 1 {
        let ty = emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                if ty != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // convert the round() input to float when it is an integer
                }
                emitter.instruction("frinta d0, d0");                           // round to nearest with ties away from zero on AArch64
            }
            Arch::X86_64 => {
                if ty != PhpType::Float {
                    emitter.instruction("cvtsi2sd xmm0, rax");                  // convert the round() input to float when it is an integer
                }
                emitter.instruction("call round");                              // delegate PHP-compatible nearest-integer rounding to libc round() on linux-x86_64
            }
        }
    } else {
        let ty = emit_expr(&args[0], emitter, ctx, data);
        match emitter.target.arch {
            Arch::AArch64 => {
                if ty != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // convert the round() value to float when it is an integer
                }
                emitter.instruction("str d0, [sp, #-16]!");                     // preserve the original value while computing the precision multiplier

                let t1 = emit_expr(&args[1], emitter, ctx, data);
                if t1 == PhpType::Float {
                    emitter.instruction("fcvtzs x0, d0");                       // convert the precision argument from float to integer before pow()
                }

                emitter.instruction("scvtf d1, x0");                            // convert the integer precision into the pow() exponent floating-point register
                emitter.instruction("str d1, [sp, #-16]!");                     // preserve the floating precision exponent while materializing the pow() base
                emitter.instruction("fmov d0, #10.0");                          // materialize 10.0 as the pow() base for precision scaling
                emitter.instruction("ldr d1, [sp], #16");                       // restore the floating precision exponent into the second pow() argument register
                emitter.bl_c("pow");                                            // compute 10^precision through the C library pow() helper

                emitter.instruction("ldr d1, [sp], #16");                       // restore the original value after pow() returns the precision multiplier
                emitter.instruction("fmul d1, d1, d0");                         // scale the original value by the precision multiplier before rounding
                emitter.instruction("str d0, [sp, #-16]!");                     // preserve the precision multiplier for the final division step
                emitter.instruction("frinta d0, d1");                           // round the scaled value to the nearest integer with ties away from zero
                emitter.instruction("ldr d1, [sp], #16");                       // restore the precision multiplier for the final division step
                emitter.instruction("fdiv d0, d0, d1");                         // divide the rounded scaled value back down by the precision multiplier
            }
            Arch::X86_64 => {
                if ty != PhpType::Float {
                    emitter.instruction("cvtsi2sd xmm0, rax");                  // convert the round() value to float when it is an integer
                }
                abi::emit_push_float_reg(emitter, "xmm0");                      // preserve the original value while computing the precision multiplier

                let t1 = emit_expr(&args[1], emitter, ctx, data);
                if t1 == PhpType::Float {
                    emitter.instruction("cvttsd2si rax, xmm0");                 // convert the precision argument from float to integer before pow()
                }

                emitter.instruction("cvtsi2sd xmm1, rax");                      // convert the integer precision into the second pow() floating-point argument register
                emitter.instruction("mov rax, 0x4024000000000000");             // materialize the IEEE-754 bit pattern for 10.0 before calling pow()
                emitter.instruction("movq xmm0, rax");                          // move 10.0 into the first pow() floating-point argument register
                emitter.instruction("call pow");                                // compute 10^precision through the libc pow() helper on linux-x86_64
                abi::emit_pop_float_reg(emitter, "xmm1");                       // restore the original value into the left-hand floating-point scratch register
                emitter.instruction("mulsd xmm1, xmm0");                        // scale the original value by the precision multiplier before rounding
                abi::emit_push_float_reg(emitter, "xmm0");                      // preserve the precision multiplier for the final division step
                emitter.instruction("movsd xmm0, xmm1");                        // move the scaled value into the first libc round() floating-point argument register
                emitter.instruction("call round");                              // round the scaled value through libc round() to preserve PHP rounding semantics
                abi::emit_pop_float_reg(emitter, "xmm1");                       // restore the precision multiplier into the left-hand floating-point scratch register
                emitter.instruction("divsd xmm0, xmm1");                        // divide the rounded scaled value back down by the precision multiplier
            }
        }
    }

    Some(PhpType::Float)
}
