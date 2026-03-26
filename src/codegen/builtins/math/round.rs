use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
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
        // -- simple round with no precision --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }        // convert signed int to float
        emitter.instruction("frinta d0, d0");                                   // round to nearest, ties away from zero
    } else {
        // -- round with precision: round(value * 10^precision) / 10^precision --
        let ty = emit_expr(&args[0], emitter, ctx, data);
        if ty != PhpType::Float {
            emitter.instruction("scvtf d0, x0");                                // convert value to float if int
        }
        emitter.instruction("str d0, [sp, #-16]!");                             // push value onto stack

        let t1 = emit_expr(&args[1], emitter, ctx, data);
        if t1 == PhpType::Float {
            emitter.instruction("fcvtzs x0, d0");                               // convert precision from float to int
        }

        // -- compute 10^precision using _pow --
        emitter.instruction("scvtf d1, x0");                                    // convert precision int to float for pow()
        emitter.instruction("str d1, [sp, #-16]!");                             // push precision as float onto stack
        emitter.instruction("fmov d0, #10.0");                                  // d0 = 10.0 (base)
        emitter.instruction("ldr d1, [sp], #16");                               // pop precision into d1 (exponent)
        emitter.instruction("bl _pow");                                         // call pow(10.0, precision) → d0 = multiplier

        // -- multiply value by multiplier, round, divide --
        emitter.instruction("ldr d1, [sp], #16");                               // pop original value into d1
        emitter.instruction("fmul d1, d1, d0");                                 // d1 = value * 10^precision
        emitter.instruction("str d0, [sp, #-16]!");                             // push multiplier for later division
        emitter.instruction("frinta d0, d1");                                   // round scaled value to nearest integer
        emitter.instruction("ldr d1, [sp], #16");                               // pop multiplier back into d1
        emitter.instruction("fdiv d0, d0, d1");                                 // d0 = rounded / 10^precision
    }

    Some(PhpType::Float)
}
