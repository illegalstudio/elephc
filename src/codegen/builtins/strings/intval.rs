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
    emitter.comment("intval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty == PhpType::Str {
        // -- convert string to integer --
        emitter.instruction("bl __rt_atoi");                            // call runtime: parse string as integer into x0
    }
    Some(PhpType::Int)
}
