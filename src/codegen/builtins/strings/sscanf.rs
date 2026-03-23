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
    emitter.comment("sscanf()");
    // sscanf($string, $format) → returns array of matched values as strings
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push input string
    emit_expr(&args[1], emitter, ctx, data);
    // x1/x2 = format string
    emitter.instruction("mov x3, x1");                                          // x3 = format ptr
    emitter.instruction("mov x4, x2");                                          // x4 = format len
    emitter.instruction("ldp x1, x2, [sp], #16");                               // pop input string
    // x1/x2 = input, x3/x4 = format
    emitter.instruction("bl __rt_sscanf");                                      // call runtime: parse string → x0=array ptr
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
