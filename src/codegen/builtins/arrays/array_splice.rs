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
    emitter.comment("array_splice()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let uses_refcounted_runtime =
        matches!(&arr_ty, PhpType::Array(inner) if inner.is_refcounted());
    // -- save array pointer, evaluate offset --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emit_expr(&args[1], emitter, ctx, data);
    if args.len() > 2 {
        // -- save offset, evaluate length --
        emitter.instruction("str x0, [sp, #-16]!");                             // push offset onto stack
        emit_expr(&args[2], emitter, ctx, data);
        // -- set up three-arg call: array, offset, length --
        emitter.instruction("mov x2, x0");                                      // move length to x2 (third arg)
        emitter.instruction("ldr x1, [sp], #16");                               // pop offset into x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
    } else {
        // -- set up two-arg call: array, offset (remove rest) --
        emitter.instruction("mov x1, x0");                                      // move offset to x1 (second arg)
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer into x0 (first arg)
        emitter.instruction("mov x2, #-1");                                     // length = -1 signals "remove until end"
    }
    // -- call runtime to splice array --
    emitter.instruction(if uses_refcounted_runtime {
        "bl __rt_array_splice_refcounted"
    } else {
        "bl __rt_array_splice"
    }); // call runtime: splice array → x0=removed elements array

    match arr_ty {
        PhpType::Array(inner) => Some(PhpType::Array(inner)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
