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
    emitter.comment("feof()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to check end-of-file --
    emitter.instruction("bl __rt_feof");                                        // check EOF: x0=fd → x0=bool (1=eof, 0=not)
    Some(PhpType::Bool)
}
