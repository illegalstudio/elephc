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
    emitter.comment("array_filter()");

    // -- evaluate the array argument (first arg) --
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    // -- resolve callback function address at compile time --
    let func_name = match &args[1].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("array_filter() callback must be a string literal"),
    };
    let label = format!("_fn_{}", func_name);
    emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
    emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback function

    // -- call runtime: x0=callback_addr, x1=array_ptr --
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    emitter.instruction("bl __rt_array_filter");                                // call runtime: filter array → x0=new array

    match arr_ty {
        PhpType::Array(elem_ty) => Some(PhpType::Array(elem_ty)),
        _ => Some(PhpType::Array(Box::new(PhpType::Int))),
    }
}
