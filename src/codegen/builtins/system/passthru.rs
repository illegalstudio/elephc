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
    emitter.comment("passthru()");
    // -- evaluate command string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- null-terminate and call libc system() which outputs directly to stdout --
    emitter.instruction("bl __rt_cstr");                                        // null-terminate command string → x0=cstr
    emitter.bl_c("system");                                          // execute command, output goes directly to stdout
    Some(PhpType::Void)
}
