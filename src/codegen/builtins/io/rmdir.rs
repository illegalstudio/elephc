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
    emitter.comment("rmdir()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=path ptr, x2=path len
    emitter.instruction("bl __rt_rmdir");                                       // call runtime: remove directory → x0=bool
    Some(PhpType::Bool)
}
