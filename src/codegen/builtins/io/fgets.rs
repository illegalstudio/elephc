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
    emitter.comment("fgets()");
    // -- evaluate fd argument --
    emit_expr(&args[0], emitter, ctx, data);
    // -- call runtime to read a line --
    emitter.instruction("bl __rt_fgets");                                       // read line: x0=fd → x1/x2=string
    Some(PhpType::Str)
}
