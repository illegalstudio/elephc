use super::store_mutating_arg::emit_store_mutating_arg;
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
    emitter.comment("array_push()");
    let _arr_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save array pointer, evaluate value to push --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack
    let val_ty = emit_expr(&args[1], emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");                                   // pop saved array pointer into x9
    match &val_ty {
        PhpType::Int | PhpType::Bool => {
            // -- push integer/bool value onto array --
            emitter.instruction("mov x1, x0");                                  // move integer value to x1 (second arg)
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append integer to array
        }
        PhpType::Float => {
            // -- push float value onto array (store as 8-byte int via bit cast) --
            emitter.instruction("fmov x1, d0");                                 // move float bits to integer register
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0 (first arg)
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: append float bits as 8 bytes
        }
        PhpType::Str => {
            // -- push string to array (push_str persists to heap internally) --
            emitter.instruction("mov x0, x9");                                  // move array pointer to x0
            emitter.instruction("bl __rt_array_push_str");                      // call runtime: persist + append string to array
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            // -- push nested refcounted pointer onto array --
            emitter.instruction("mov x1, x0");                                  // move pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_refcounted");               // append retained pointer and stamp array metadata
        }
        PhpType::Callable => {
            // -- push callable pointer onto array as a plain 8-byte scalar --
            emitter.instruction("mov x1, x0");                                  // move callable pointer value to x1
            emitter.instruction("mov x0, x9");                                  // move outer array pointer to x0
            emitter.instruction("bl __rt_array_push_int");                      // append function pointer bits as a plain scalar slot
        }
        _ => {}
    }

    // -- update stored array pointer (may have changed due to COW splitting or reallocation) --
    emit_store_mutating_arg(emitter, ctx, &args[0]);

    Some(PhpType::Void)
}
