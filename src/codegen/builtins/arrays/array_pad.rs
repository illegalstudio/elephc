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
    emitter.comment("array_pad()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    // -- save array pointer, evaluate target size --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    // -- save target size, evaluate pad value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push target size onto stack
    emit_expr(&args[2], emitter, ctx, data);
    // -- set up three-arg call: array, size, value --
    emitter.instruction("mov x2, x0");                                          // move pad value to x2 (third arg)
    emitter.instruction("ldr x1, [sp], #16");                                   // pop target size into x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_pad_refcounted"
    } else {
        "bl __rt_array_pad"
    };
    emitter.instruction(runtime_call);                                          // call runtime: pad array → x0=new array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
