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
    emitter.comment("array_search()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save needle, evaluate array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push needle value onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime: x0=array, x1=needle --
    emitter.instruction("ldr x1, [sp], #16");                                   // pop needle value into x1
    // x0 already has array pointer
    emitter.instruction("bl __rt_array_search");                                // call runtime: search array → x0=index or -1

    Some(PhpType::Int)
}
