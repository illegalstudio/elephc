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
    emitter.comment("range()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save start value, evaluate end value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start value onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to create array from start to end --
    emitter.instruction("mov x1, x0");                                          // move end value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start value into x0 (first arg)
    emitter.instruction("bl __rt_range");                                       // call runtime: create range → x0=new array

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
