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
    emitter.comment("ptr_is_null()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- check if pointer is null (0x0) --
    emitter.instruction("cmp x0, #0");                                          // compare pointer against null
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if null, 0 otherwise
    Some(PhpType::Bool)
}
