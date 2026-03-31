use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_walk()");

    // -- evaluate the array argument (first arg) --
    emit_expr(&args[0], emitter, ctx, data);

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    // -- resolve callback function address --
    let is_closure = matches!(&args[1].kind, ExprKind::Closure { .. });
    if is_closure {
        emit_expr(&args[1], emitter, ctx, data);
        emitter.instruction("mov x19, x0");                                     // move closure address to x19
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                              // load callback address from variable
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_walk() callback must be a string literal, closure, or callable variable"),
        };
        let label = function_symbol(&func_name);
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));              // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));       // resolve full address of callback function
    }

    // -- call runtime: x0=callback_addr, x1=array_ptr --
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    emitter.instruction("bl __rt_array_walk");                                  // call runtime: walk array calling callback on each element

    Some(PhpType::Void)
}
