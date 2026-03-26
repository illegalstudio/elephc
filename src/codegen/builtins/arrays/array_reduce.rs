use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_reduce()");

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(&args[1].kind, ExprKind::Closure { .. });
    if is_closure {
        // Evaluate closure → x0 = function address
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // save callback address on stack
    }

    // -- evaluate the array argument (first arg) --
    emit_expr(&args[0], emitter, ctx, data);

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    if is_closure {
        // -- load callback address from saved stack slot --
        emitter.instruction("ldr x19, [sp, #16]");                              // peek callback address (saved before array)
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        // Callable variable — load from stack slot
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                              // load callback address from variable
    } else {
        // String literal — resolve at compile time
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_reduce() callback must be a string literal, closure, or callable variable"),
        };
        let label = format!("_fn_{}", func_name);
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));              // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));       // resolve full address of callback function
    }

    // -- evaluate initial value (third arg) --
    emit_expr(&args[2], emitter, ctx, data);

    // -- call runtime: x0=callback_addr, x1=array_ptr, x2=initial --
    emitter.instruction("mov x2, x0");                                          // x2 = initial value
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    if is_closure {
        emitter.instruction("add sp, sp, #16");                                 // discard saved callback address
    }
    emitter.instruction("bl __rt_array_reduce");                                // call runtime: reduce array → x0=accumulated result

    Some(PhpType::Int)
}
