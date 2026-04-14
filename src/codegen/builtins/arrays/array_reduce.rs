use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
    emitter.comment("array_reduce()");
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let callback_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let initial_arg_reg = abi::int_arg_reg_name(emitter.target, 2);

    // -- evaluate the callback argument (may be a string literal or closure) --
    let is_closure = matches!(
        &args[1].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    if is_closure {
        emit_expr(&args[1], emitter, ctx, data);
        abi::emit_push_reg(emitter, result_reg);                                // save the synthesized callback address on the temporary stack
    }

    // -- evaluate the array argument, then resolve the callback address into the target scratch register --
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, result_reg);                                    // push the source array pointer onto the temporary stack

    if is_closure {
    } else if let ExprKind::Variable(var_name) = &args[1].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, call_reg, offset);                         // load the callback address from the callable variable slot
    } else {
        let func_name = match &args[1].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("array_reduce() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        abi::emit_symbol_address(emitter, call_reg, &label);                         // materialize the callback function address in the nested-call scratch register
    }

    // -- evaluate initial value (third arg) --
    emit_expr(&args[2], emitter, ctx, data);
    emitter.instruction(&format!("mov {}, {}", initial_arg_reg, result_reg));   // place the initial accumulator in the third runtime argument register

    // -- place callback and array pointer into the runtime argument registers --
    if is_closure {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the source array pointer into the second runtime argument register
        abi::emit_pop_reg(emitter, callback_arg_reg);                            // pop the synthesized callback address into the first runtime argument register
    } else {
        abi::emit_pop_reg(emitter, array_arg_reg);                               // pop the source array pointer into the second runtime argument register
        emitter.instruction(&format!("mov {}, {}", callback_arg_reg, call_reg)); // move the callback function address into the first runtime argument register
    }
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_call_label(emitter, "__rt_array_reduce");                      // call the x86_64 callback-driven reduce runtime helper
    } else {
        emitter.instruction("bl __rt_array_reduce");                            // call the ARM64 callback-driven reduce runtime helper
    }

    Some(PhpType::Int)
}
