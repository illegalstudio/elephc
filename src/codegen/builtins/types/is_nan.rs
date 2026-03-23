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
    emitter.comment("is_nan()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- NaN is the only value that does not equal itself --
    if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }            // convert int to float if needed
    emitter.instruction("fcmp d0, d0");                                         // compare float with itself (NaN != NaN)
    emitter.instruction("cset x0, vs");                                         // x0 = 1 if unordered (NaN), 0 otherwise
    Some(PhpType::Bool)
}
