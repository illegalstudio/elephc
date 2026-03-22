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
    emitter.comment("str_split()");
    emit_expr(&args[0], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                    // push string
    if args.len() >= 2 {
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov x3, x0");                             // chunk length
    } else {
        emitter.instruction("mov x3, #1");                              // default chunk = 1
    }
    emitter.instruction("ldp x1, x2, [sp], #16");                      // pop string
    emitter.instruction("bl __rt_str_split");                           // call runtime: split string into chunks
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
