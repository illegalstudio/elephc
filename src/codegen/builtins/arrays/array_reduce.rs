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

    // -- evaluate the array argument (first arg) --
    emit_expr(&args[0], emitter, ctx, data);

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    // -- resolve callback function address at compile time --
    let func_name = match &args[1].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("array_reduce() callback must be a string literal"),
    };
    let label = format!("_fn_{}", func_name);
    emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
    emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback function

    // -- evaluate initial value (third arg) --
    emit_expr(&args[2], emitter, ctx, data);

    // -- call runtime: x0=callback_addr, x1=array_ptr, x2=initial --
    emitter.instruction("mov x2, x0");                                          // x2 = initial value
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    emitter.instruction("bl __rt_array_reduce");                                // call runtime: reduce array → x0=accumulated result

    Some(PhpType::Int)
}
