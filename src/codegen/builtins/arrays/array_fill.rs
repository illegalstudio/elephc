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
    emitter.comment("array_fill()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save start index, evaluate count --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start index onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- save count, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push count onto stack
    emit_expr(&args[2], emitter, ctx, data);
    // -- set up three-arg call: start, count, value --
    emitter.instruction("mov x2, x0");                                          // move fill value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop count into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start index into x0 (first arg)
    emitter.instruction("bl __rt_array_fill");                                  // call runtime: fill array → x0=new array

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
