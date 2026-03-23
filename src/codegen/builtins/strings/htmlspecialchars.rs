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
    emitter.comment("htmlspecialchars()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("bl __rt_htmlspecialchars");                            // call runtime: convert special chars to HTML entities
    Some(PhpType::Str)
}
