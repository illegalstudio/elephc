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
    emitter.comment("md5()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=string ptr, x2=string len
    emitter.instruction("bl __rt_md5");                                         // call runtime: compute MD5 hash → x1/x2=hex string
    Some(PhpType::Str)
}
