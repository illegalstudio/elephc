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
    emitter.comment("floatval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        // -- convert integer to double-precision float --
        emitter.instruction("scvtf d0, x0");                            // convert signed int x0 to float d0
    }
    Some(PhpType::Float)
}
