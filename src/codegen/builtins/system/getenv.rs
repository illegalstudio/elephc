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
    emitter.comment("getenv()");
    // -- evaluate the environment variable name string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- convert to C string and call getenv --
    emitter.instruction("bl __rt_getenv");                                      // get env var: x1/x2=name → x1=ptr, x2=len
    Some(PhpType::Str)
}
