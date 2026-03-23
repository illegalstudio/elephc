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
    emitter.comment("scandir()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=dir ptr, x2=dir len
    emitter.instruction("bl __rt_scandir");                                     // call runtime: list directory entries → x0=array ptr
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
