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
    emitter.comment("abs()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty == PhpType::Float {
        // -- float absolute value --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fabs d0, d0");                             // take absolute value of the floating-point result in place
            }
            Arch::X86_64 => {
                emitter.instruction("movq r10, xmm0");                          // move the floating-point payload into a scratch integer register for sign-bit masking
                emitter.instruction("mov r11, 0x7fffffffffffffff");             // materialize a mask that clears the IEEE-754 sign bit
                emitter.instruction("and r10, r11");                            // clear the sign bit so the payload becomes its absolute value
                emitter.instruction("movq xmm0, r10");                          // move the masked floating-point payload back into the result register
            }
        }
        Some(PhpType::Float)
    } else {
        // -- integer absolute value via conditional negate --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // compare the integer value against zero
                emitter.instruction("cneg x0, x0, lt");                         // negate the integer result only when it was negative
            }
            Arch::X86_64 => {
                emitter.instruction("mov r10, rax");                            // copy the integer value into a scratch register before branchless sign handling
                emitter.instruction("sar r10, 63");                             // expand the sign bit to an all-zero or all-one mask
                emitter.instruction("xor rax, r10");                            // flip the payload bits when the original integer was negative
                emitter.instruction("sub rax, r10");                            // subtract the sign mask to finish the two's-complement absolute value
            }
        }
        Some(PhpType::Int)
    }
}
