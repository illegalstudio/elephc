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
    emitter.comment("boolval()");
    // -- convert any value to boolean (truthy/falsy) --
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("cmp x0, #0");                                          // compare value against zero
    emitter.instruction("cset x0, ne");                                         // x0 = 1 if nonzero (truthy), 0 if zero
    Some(PhpType::Bool)
}
