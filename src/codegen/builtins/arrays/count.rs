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
    emitter.comment("count()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- read element count from array header --
    emitter.instruction("ldr x0, [x0]");                                // load array length from first field of array struct

    Some(PhpType::Int)
}
