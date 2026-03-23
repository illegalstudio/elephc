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
    emitter.comment("natsort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort array using natural order algorithm --
    emitter.instruction("bl __rt_natsort");                                     // call runtime: sort array using natural ordering

    Some(PhpType::Void)
}
