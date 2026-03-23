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
    emitter.comment("filesize()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=filename ptr, x2=filename len
    emitter.instruction("bl __rt_filesize");                                    // call runtime: get file size in bytes → x0=size
    Some(PhpType::Int)
}
