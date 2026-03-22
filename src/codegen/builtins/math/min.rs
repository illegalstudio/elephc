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
    emitter.comment("min()");
    // -- evaluate first arg and push it onto the stack --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 == PhpType::Float {
        emitter.instruction("str d0, [sp, #-16]!");                     // push first arg as float
    } else {
        emitter.instruction("str x0, [sp, #-16]!");                     // push first arg as int
    }
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t0 == PhpType::Float || t1 == PhpType::Float {
        // -- float min: coerce both to float, use fmin --
        if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); } // convert second arg to float
        if t0 == PhpType::Float {
            emitter.instruction("ldr d1, [sp], #16");                   // pop first arg as float into d1
        } else {
            emitter.instruction("ldr x9, [sp], #16");                   // pop first arg as int into x9
            emitter.instruction("scvtf d1, x9");                        // convert first arg int to float
        }
        emitter.instruction("fmin d0, d1, d0");                         // d0 = minimum of d1 and d0
        Some(PhpType::Float)
    } else {
        // -- integer min: compare and conditionally select --
        emitter.instruction("ldr x1, [sp], #16");                       // pop first arg into x1
        emitter.instruction("cmp x1, x0");                              // compare first arg with second arg
        emitter.instruction("csel x0, x1, x0, lt");                     // select smaller value (x1 if x1 < x0)
        Some(PhpType::Int)
    }
}
