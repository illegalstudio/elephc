use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::array_map_callback_returns_str::callback_returns_str;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_map()");

    // -- determine callback return type at compile time --
    let returns_str = callback_returns_str(args, ctx);

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    if is_closure {
        // Evaluate closure → x0 = function address
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // save callback address on stack
    }

    // -- evaluate the array argument --
    let _arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // -- save array pointer, load callback address into x19 --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    if is_closure {
        // -- load callback address from saved stack slot --
        emitter.instruction("ldr x19, [sp, #16]");                              // peek callback address (saved before array)
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        // Callable variable — load from stack slot
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                              // load callback address from variable
    } else {
        // String literal — resolve at compile time
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_map() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        emitter.adrp("x19", &format!("{}", label));              // load page address of callback function
        emitter.add_lo12("x19", "x19", &format!("{}", label));       // resolve full address of callback function
    }

    // -- call runtime: x0=callback_addr, x1=array_ptr --
    emitter.instruction("mov x0, x19");                                         // x0 = callback function address
    emitter.instruction("ldr x1, [sp], #16");                                   // pop array pointer into x1
    if is_closure {
        emitter.instruction("add sp, sp, #16");                                 // discard saved callback address
    }

    if returns_str {
        emitter.instruction("bl __rt_array_map_str");                           // call runtime: map callback over array → x0=new string array
        Some(PhpType::Array(Box::new(PhpType::Str)))
    } else {
        emitter.instruction("bl __rt_array_map");                               // call runtime: map callback over array → x0=new array
        Some(PhpType::Array(Box::new(PhpType::Int)))
    }
}
