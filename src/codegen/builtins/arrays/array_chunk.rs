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
    emitter.comment("array_chunk()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save array pointer, evaluate chunk size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to split array into chunks --
    emitter.instruction("mov x1, x0");                                          // move chunk size to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    emitter.instruction("bl __rt_array_chunk");                                 // call runtime: chunk array → x0=array of arrays

    Some(PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Int)))))
}
