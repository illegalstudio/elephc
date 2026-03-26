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
    emitter.comment("implode()");
    // implode($glue, $array)
    emit_expr(&args[0], emitter, ctx, data);
    // -- save glue, evaluate array --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push glue ptr and length onto stack
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);
    // -- save array pointer, restore glue --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // load glue ptr and length (still on stack)

    let is_int_array = matches!(&arr_ty, PhpType::Array(inner) if matches!(inner.as_ref(), PhpType::Int | PhpType::Bool));

    if is_int_array {
        // -- integer array: call int-specific implode runtime --
        emitter.instruction("ldr x3, [sp]");                                    // load array pointer from top of stack
        emitter.instruction("add sp, sp, #32");                                 // pop array pointer and glue from stack
        emitter.instruction("bl __rt_implode_int");                             // call runtime: join int elements with glue string
    } else {
        // -- string array: call standard implode runtime --
        emitter.instruction("ldr x3, [sp]");                                    // load array pointer from top of stack
        emitter.instruction("add sp, sp, #32");                                 // pop array pointer and glue from stack
        emitter.instruction("bl __rt_implode");                                 // call runtime: join string elements with glue string
    }

    Some(PhpType::Str)
}
