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
    emitter.comment("ord()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- return ASCII value of first character --
    emitter.instruction("ldrb w0, [x1]");                               // load first byte from string ptr as unsigned int

    Some(PhpType::Int)
}
