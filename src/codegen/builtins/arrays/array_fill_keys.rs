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
    emitter.comment("array_fill_keys()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save keys array, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push keys array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to create assoc array from keys with given value --
    emitter.instruction("mov x1, x0");                                          // move fill value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop keys array pointer into x0 (first arg)
    emitter.instruction("bl __rt_array_fill_keys");                             // call runtime: fill keys → x0=new assoc array

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
