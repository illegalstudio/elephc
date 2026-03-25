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
    emitter.comment("usleep()");
    // -- evaluate microseconds argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call libc usleep (x0 = microseconds) --
    emitter.instruction("bl _usleep");                                          // sleep for x0 microseconds
    Some(PhpType::Void)
}
