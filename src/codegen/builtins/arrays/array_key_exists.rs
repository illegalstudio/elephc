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
    emitter.comment("array_key_exists()");

    // -- evaluate the array (second arg) first to get its type --
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        // -- associative array: use hash_get to check if key exists --
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer
        emit_expr(&args[0], emitter, ctx, data);
        // key is a string → result in x1/x2
        emitter.instruction("mov x3, x1");                                      // save key ptr to x3
        emitter.instruction("mov x4, x2");                                      // save key len to x4
        emitter.instruction("ldr x0, [sp], #16");                               // pop hash table pointer
        emitter.instruction("mov x1, x3");                                      // key ptr into x1
        emitter.instruction("mov x2, x4");                                      // key len into x2
        emitter.instruction("bl __rt_hash_get");                                // lookup key → x0=found (0 or 1)
        // x0 already holds the found flag (1 or 0)
    } else {
        // -- indexed array: check if integer key is in bounds --
        emitter.instruction("str x0, [sp, #-16]!");                             // push array pointer
        emit_expr(&args[0], emitter, ctx, data);
        // key is an integer → result in x0
        emitter.instruction("mov x1, x0");                                      // move key to x1
        emitter.instruction("ldr x0, [sp], #16");                               // pop array pointer
        emitter.instruction("bl __rt_array_key_exists");                        // check bounds → x0=bool
    }

    Some(PhpType::Bool)
}
