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
    emitter.comment("ceil()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- convert int to float if needed, then round toward plus infinity --
    if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert signed int to float
    emitter.instruction("frintp d0, d0");                                       // round toward plus infinity (ceil)
    Some(PhpType::Float)
}
