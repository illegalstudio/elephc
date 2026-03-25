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
    emitter.comment("preg_replace()");

    // -- evaluate subject string (arg 2) first --
    emit_expr(&args[2], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push subject ptr and len

    // -- evaluate replacement string (arg 1) --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push replacement ptr and len

    // -- evaluate pattern string (arg 0) --
    emit_expr(&args[0], emitter, ctx, data);
    // x1=pattern ptr, x2=pattern len

    // -- pop replacement into x3/x4 --
    emitter.instruction("ldp x3, x4, [sp], #16");                               // pop replacement ptr/len into x3/x4

    // -- pop subject into x5/x6 --
    emitter.instruction("ldp x5, x6, [sp], #16");                               // pop subject ptr/len into x5/x6

    // -- call runtime: x1/x2=pattern, x3/x4=replacement, x5/x6=subject --
    emitter.instruction("bl __rt_preg_replace");                                // regex replace → x1=result ptr, x2=result len

    Some(PhpType::Str)
}
