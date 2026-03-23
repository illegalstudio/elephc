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
    emitter.comment("fputcsv()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save fd, evaluate array arg --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push file descriptor onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x1, x0");                                          // move array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop file descriptor into x0
    emitter.instruction("bl __rt_fputcsv");                                     // call runtime: write array as CSV line → x0=bytes written
    Some(PhpType::Int)
}
