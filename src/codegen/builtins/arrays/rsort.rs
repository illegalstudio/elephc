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
    emitter.comment("rsort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort integer array in descending order --
    emitter.instruction("bl __rt_rsort_int");                                   // call runtime: sort array of integers descending

    Some(PhpType::Void)
}
