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
    emitter.comment("arsort()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- sort associative array by values descending, maintaining key association --
    emitter.instruction("bl __rt_arsort");                                      // call runtime: sort assoc array by values descending

    Some(PhpType::Void)
}
