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
    emitter.comment("strtoupper()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- convert all characters to uppercase --
    emitter.instruction("bl __rt_strtoupper");                                  // call runtime: uppercase string in-place, result in x1/x2

    Some(PhpType::Str)
}
