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
    emitter.comment("atan2()");
    // -- evaluate y (first arg) --
    let t0 = emit_expr(&args[0], emitter, ctx, data);
    if t0 != PhpType::Float {
        emitter.instruction("scvtf d0, x0");                                    // convert y to float if int
    }
    emitter.instruction("str d0, [sp, #-16]!");                                 // save y on stack
    // -- evaluate x (second arg) --
    let t1 = emit_expr(&args[1], emitter, ctx, data);
    if t1 != PhpType::Float {
        emitter.instruction("scvtf d0, x0");                                    // convert x to float if int
    }
    emitter.instruction("fmov d1, d0");                                         // move x to d1 (second arg)
    emitter.instruction("ldr d0, [sp], #16");                                   // restore y to d0 (first arg)
    emitter.bl_c("atan2");                                           // call libc atan2(y, x) → d0
    Some(PhpType::Float)
}
