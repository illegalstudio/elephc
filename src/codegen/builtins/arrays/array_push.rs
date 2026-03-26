use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

// @todo: add support for array_push() with floats, booleans and other types
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_push()");
    emit_expr(&args[0], emitter, ctx, data);
    // -- save array pointer, evaluate value to push --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let val_ty = emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    match &val_ty {
        PhpType::Int => {
            // -- push integer value onto array --
            emitter.instruction("mov x1, x0");                                  // move integer value to x1 (second arg)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to array
        }
        PhpType::Str => {
            // -- push string to array (push_str persists to heap internally) --
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // -- push nested array pointer onto array --
            emitter.instruction("mov x1, x0");                                  // move nested array pointer to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append pointer (8 bytes, same as int)
        }
        _ => {}
    }

    // -- update stored array pointer (may have changed due to reallocation) --
    if let ExprKind::Variable(name) = &args[0].kind {
        if let Some(var) = ctx.variables.get(name) {
            let offset = var.stack_offset;
            abi::store_at_offset(emitter, "x0", offset);                        // save possibly-new array pointer
        }
    }

    Some(PhpType::Void)
}
