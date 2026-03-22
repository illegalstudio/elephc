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
    emitter.comment("str_replace()");
    // str_replace($search, $replace, $subject)
    emit_expr(&args[0], emitter, ctx, data);
    // -- save search and replace strings, evaluate subject --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push search ptr and length onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push replace ptr and length onto stack
    emit_expr(&args[2], emitter, ctx, data);
    // -- arrange all args into registers for runtime call --
    emitter.instruction("mov x5, x1");                                  // move subject pointer to x5
    emitter.instruction("mov x6, x2");                                  // move subject length to x6
    emitter.instruction("ldp x3, x4, [sp], #16");                       // pop replace ptr into x3, length into x4
    emitter.instruction("ldp x1, x2, [sp], #16");                       // pop search ptr into x1, length into x2
    emitter.instruction("bl __rt_str_replace");                         // call runtime: replace all occurrences, result in x1/x2

    Some(PhpType::Str)
}
