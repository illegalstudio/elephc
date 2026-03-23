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
    emitter.comment("implode()");
    // implode($glue, $array)
    emit_expr(&args[0], emitter, ctx, data);
    // -- save glue, evaluate array --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push glue ptr and length onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x3, x0");                                          // move array pointer to x3
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop glue ptr into x1, length into x2
    emitter.instruction("bl __rt_implode");                                     // call runtime: join array elements with glue string

    Some(PhpType::Str)
}
