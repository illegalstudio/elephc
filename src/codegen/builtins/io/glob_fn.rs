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
    emitter.comment("glob()");
    emit_expr(&args[0], emitter, ctx, data);
    // x1=pattern ptr, x2=pattern len
    emitter.instruction("bl __rt_glob");                                        // call runtime: match files by glob pattern → x0=array ptr
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
