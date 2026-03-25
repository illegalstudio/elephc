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
    emitter.comment("strtotime()");

    // -- evaluate date string argument --
    emit_expr(&args[0], emitter, ctx, data);
    // x1=string ptr, x2=string len

    // -- call runtime to parse date string and return timestamp --
    emitter.instruction("bl __rt_strtotime");                                   // parse date string → x0=timestamp

    Some(PhpType::Int)
}
