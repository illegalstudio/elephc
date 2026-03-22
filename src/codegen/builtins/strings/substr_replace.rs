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
    emitter.comment("substr_replace()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push subject string
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push replacement string
    emit_expr(&args[2], emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");                                 // push offset
    if args.len() >= 4 {
        emit_expr(&args[3], emitter, ctx, data);
        emitter.instruction("mov x7, x0");                                      // length arg
    } else {
        emitter.instruction("mov x7, #-1");                                     // sentinel: replace to end
    }
    emitter.instruction("ldr x0, [sp], #16");                                   // pop offset
    emitter.instruction("ldp x3, x4, [sp], #16");                               // pop replacement
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop subject
    // x1/x2=subject, x3/x4=replacement, x0=offset, x7=length
    emitter.instruction("bl __rt_substr_replace");                              // call runtime: replace substring
    Some(PhpType::Str)
}
