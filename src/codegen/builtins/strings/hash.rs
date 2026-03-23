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
    emitter.comment("hash()");
    // hash($algo, $data) — evaluate algo string first
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                            // push algorithm string ptr/len
    emit_expr(&args[1], emitter, ctx, data);
    // x1/x2 = data string
    emitter.instruction("mov x3, x1");                                         // x3 = data ptr
    emitter.instruction("mov x4, x2");                                         // x4 = data len
    emitter.instruction("ldp x1, x2, [sp], #16");                              // pop algorithm ptr/len
    // x1/x2 = algo, x3/x4 = data
    emitter.instruction("bl __rt_hash");                                        // call runtime: compute hash → x1/x2=hex string
    Some(PhpType::Str)
}
