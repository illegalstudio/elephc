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
    emitter.comment("strlen()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- return the string length as an integer --
    emitter.instruction("mov x0, x2");                                  // move string length to return register

    Some(PhpType::Int)
}
