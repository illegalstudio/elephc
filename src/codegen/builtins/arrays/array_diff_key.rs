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
    emitter.comment("array_diff_key()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save first array, evaluate second array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push first array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- call runtime to compute key difference --
    emitter.instruction("mov x1, x0");                                          // move second array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop first array pointer into x0
    emitter.instruction("bl __rt_array_diff_key");                              // call runtime: diff by keys → x0=new array

    Some(arr_ty)
}
