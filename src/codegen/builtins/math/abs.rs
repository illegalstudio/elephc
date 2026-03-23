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
    emitter.comment("abs()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty == PhpType::Float {
        // -- float absolute value --
        emitter.instruction("fabs d0, d0");                                     // take absolute value of float in d0
        Some(PhpType::Float)
    } else {
        // -- integer absolute value via conditional negate --
        emitter.instruction("cmp x0, #0");                                      // compare integer value against zero
        emitter.instruction("cneg x0, x0, lt");                                 // negate x0 if it was negative (lt)
        Some(PhpType::Int)
    }
}
