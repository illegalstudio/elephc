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
    emitter.comment("preg_split()");

    // -- evaluate subject string (arg 1) first --
    emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("stp x1, x2, [sp, #-16]!");                                // push subject ptr and len

    // -- evaluate pattern string (arg 0) --
    emit_expr(&args[0], emitter, ctx, data);
    // x1=pattern ptr, x2=pattern len

    // -- pop subject into x3/x4 --
    emitter.instruction("ldp x3, x4, [sp], #16");                                  // pop subject ptr/len into x3/x4

    // -- call runtime: x1/x2=pattern, x3/x4=subject --
    emitter.instruction("bl __rt_preg_split");                                      // regex split → x0=array pointer

    Some(PhpType::Array(Box::new(PhpType::Str)))
}
