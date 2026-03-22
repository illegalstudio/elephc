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
    emitter.comment("intdiv()");
    // -- integer division: dividend / divisor --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                         // push dividend onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x1, [sp], #16");                           // pop dividend into x1
    emitter.instruction("sdiv x0, x1, x0");                             // x0 = x1 / x0 (signed integer divide)
    Some(PhpType::Int)
}
