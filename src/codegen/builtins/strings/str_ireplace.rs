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
    emitter.comment("str_ireplace()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push search string
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push replace string
    emit_expr(&args[2], emitter, ctx, data);
    emitter.instruction("mov x5, x1");                                 // subject ptr
    emitter.instruction("mov x6, x2");                                 // subject len
    emitter.instruction("ldp x3, x4, [sp], #16");                      // pop replace
    emitter.instruction("ldp x1, x2, [sp], #16");                      // pop search
    emitter.instruction("bl __rt_str_ireplace");                        // call runtime: case-insensitive replace
    Some(PhpType::Str)
}
