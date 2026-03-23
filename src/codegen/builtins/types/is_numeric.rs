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
    emitter.comment("is_numeric()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- return true if type is int or float --
    let val = if matches!(ty, PhpType::Int | PhpType::Float) { 1 } else { 0 };
    emitter.instruction(&format!("mov x0, #{}", val));                          // set result: 1 if numeric, 0 otherwise
    Some(PhpType::Bool)
}
