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
    emitter.comment("file_get_contents()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=filename ptr, x2=filename len
    emitter.instruction("bl __rt_file_get_contents");                           // call runtime: read file contents → x1/x2=string
    Some(PhpType::Str)
}
