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
    emitter.comment("str_starts_with()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save haystack, evaluate prefix --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push haystack ptr and length onto stack
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("mov x3, x1");                                  // move prefix pointer to x3
    emitter.instruction("mov x4, x2");                                  // move prefix length to x4
    emitter.instruction("ldp x1, x2, [sp], #16");                       // pop haystack ptr into x1, length into x2
    emitter.instruction("bl __rt_str_starts_with");                     // call runtime: check if haystack starts with prefix

    Some(PhpType::Bool)
}
