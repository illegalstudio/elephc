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
    emitter.comment("pow()");
    // -- evaluate base, save it, evaluate exponent, call C pow() --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert base to float if int
    emitter.instruction("str d0, [sp, #-16]!");                                 // push base onto stack (pre-decrement)
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert exponent to float if int
    emitter.instruction("fmov d1, d0");                                         // move exponent to d1 (second arg)
    emitter.instruction("ldr d0, [sp], #16");                                   // pop base into d0 (first arg)
    emitter.bl_c("pow");                                             // call C library pow(base, exponent)
    Some(PhpType::Float)
}
