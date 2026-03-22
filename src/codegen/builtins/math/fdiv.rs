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
    emitter.comment("fdiv()");
    // -- floating-point division --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert dividend to float if int
    emitter.instruction("str d0, [sp, #-16]!");                         // push dividend onto stack
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t1 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert divisor to float if int
    emitter.instruction("ldr d1, [sp], #16");                           // pop dividend into d1
    emitter.instruction("fdiv d0, d1, d0");                             // d0 = d1 / d0 (float division)
    Some(PhpType::Float)
}
